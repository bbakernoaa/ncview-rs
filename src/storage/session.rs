//! Session state persistence.
//!
//! Stores and restores session context (variable, slice indices, scaling,
//! masking, zoom bounds, color maps, grid mode, etc.) for a specific dataset or list of datasets.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::app::{AppState, ColorScaleScope, GridMode, ScaleMode};
use crate::data::slice::Bounds;
use crate::render::colors::Palette;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    pub selected_variable: Option<String>,
    pub time_index: usize,
    pub depth_index: usize,
    pub active_file: usize,
    pub palette_name: String,
    pub palette_reversed: bool,
    pub scale_mode: SavedScaleMode,
    pub color_scale_scope: SavedColorScaleScope,
    pub limits: Option<(f64, f64)>,
    pub limits_manual: bool,
    pub filter_range: Option<(f64, f64)>,
    pub zoom_bounds: Option<(usize, usize, usize, usize)>,
    pub x_axis: Option<String>,
    pub y_axis: Option<String>,
    pub grid_mode: SavedGridMode,
    pub show_land_borders: bool,
    pub selected_point: Option<(usize, usize)>,
    pub selected_points: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SavedScaleMode {
    Linear,
    Log,
}

impl From<ScaleMode> for SavedScaleMode {
    fn from(mode: ScaleMode) -> Self {
        match mode {
            ScaleMode::Linear => SavedScaleMode::Linear,
            ScaleMode::Log => SavedScaleMode::Log,
        }
    }
}

impl From<SavedScaleMode> for ScaleMode {
    fn from(mode: SavedScaleMode) -> Self {
        match mode {
            SavedScaleMode::Linear => ScaleMode::Linear,
            SavedScaleMode::Log => ScaleMode::Log,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SavedColorScaleScope {
    CurrentView,
    GlobalView,
}

impl From<ColorScaleScope> for SavedColorScaleScope {
    fn from(scope: ColorScaleScope) -> Self {
        match scope {
            ColorScaleScope::CurrentView => SavedColorScaleScope::CurrentView,
            ColorScaleScope::GlobalView => SavedColorScaleScope::GlobalView,
        }
    }
}

impl From<SavedColorScaleScope> for ColorScaleScope {
    fn from(scope: SavedColorScaleScope) -> Self {
        match scope {
            SavedColorScaleScope::CurrentView => ColorScaleScope::CurrentView,
            SavedColorScaleScope::GlobalView => ColorScaleScope::GlobalView,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SavedGridMode {
    Logical,
    Projected,
}

impl From<GridMode> for SavedGridMode {
    fn from(mode: GridMode) -> Self {
        match mode {
            GridMode::Logical => SavedGridMode::Logical,
            GridMode::Projected => SavedGridMode::Projected,
        }
    }
}

impl From<SavedGridMode> for GridMode {
    fn from(mode: SavedGridMode) -> Self {
        match mode {
            SavedGridMode::Logical => GridMode::Logical,
            SavedGridMode::Projected => GridMode::Projected,
        }
    }
}

impl SessionState {
    pub fn capture(app_state: &AppState, active_file: usize) -> Self {
        let view = &app_state.view;
        let palette_name = view.palette.name().to_string();
        let palette_reversed = view.palette.is_reversed();
        let zoom_bounds = view
            .zoom_bounds
            .map(|b| (b.row_start, b.row_end, b.col_start, b.col_end));

        Self {
            selected_variable: view.selected_variable.clone(),
            time_index: view.time_index,
            depth_index: view.depth_index,
            active_file,
            palette_name,
            palette_reversed,
            scale_mode: view.scale_mode.into(),
            color_scale_scope: view.color_scale_scope.into(),
            limits: view.limits,
            limits_manual: view.limits_manual,
            filter_range: view.filter_range,
            zoom_bounds,
            x_axis: view.x_axis.clone(),
            y_axis: view.y_axis.clone(),
            grid_mode: view.grid_mode.into(),
            show_land_borders: view.show_land_borders,
            selected_point: view.selected_point,
            selected_points: view.selected_points.clone(),
        }
    }

    pub fn apply_to(&self, app_state: &mut AppState, catalog: &[Palette]) -> usize {
        let view = &mut app_state.view;
        if let Some(var) = &self.selected_variable {
            view.selected_variable = Some(var.clone());
        }
        view.time_index = self.time_index;
        view.depth_index = self.depth_index;
        view.depth_cursor = self.depth_index;

        // Find palette in catalog by name
        let palette = catalog
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(&self.palette_name))
            .cloned()
            .unwrap_or(Palette::Viridis);

        view.palette = if self.palette_reversed {
            palette.toggle_reversed()
        } else {
            palette
        };

        view.scale_mode = self.scale_mode.into();
        view.color_scale_scope = self.color_scale_scope.into();
        view.limits = self.limits;
        view.limits_manual = self.limits_manual;
        view.filter_range = self.filter_range;

        if let Some((r_start, r_end, c_start, c_end)) = self.zoom_bounds {
            view.zoom_bounds = Bounds::new(r_start, r_end, c_start, c_end).ok();
        } else {
            view.zoom_bounds = None;
        }

        view.x_axis = self.x_axis.clone();
        view.y_axis = self.y_axis.clone();
        view.grid_mode = self.grid_mode.into();
        view.show_land_borders = self.show_land_borders;
        view.selected_point = self.selected_point;
        view.selected_points = self.selected_points.clone();

        self.active_file
    }
}

/// Compute a key uniquely identifying a dataset or dataset list.
pub fn session_key_for_datasets(datasets: &[String]) -> String {
    let mut normalized = Vec::with_capacity(datasets.len());
    for ds in datasets {
        if let Ok(canonical) = fs::canonicalize(ds) {
            normalized.push(canonical.to_string_lossy().to_string());
        } else {
            normalized.push(ds.clone());
        }
    }
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Directory where session state files are stored.
pub fn session_dir() -> PathBuf {
    if let Some(dir) = env::var_os("NCVIEW_SESSION_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(dir).join("ncview").join("sessions");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("ncview")
            .join("sessions");
    }
    env::temp_dir().join("ncview").join("sessions")
}

/// Load saved session state for the given dataset(s) if present.
pub fn load_session(datasets: &[String]) -> Option<SessionState> {
    if datasets.is_empty() {
        return None;
    }
    let key = session_key_for_datasets(datasets);
    let session_path = session_dir().join(format!("{key}.json"));
    let contents = fs::read_to_string(session_path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Save session state for the given dataset(s).
pub fn save_session(
    datasets: &[String],
    app_state: &AppState,
    active_file: usize,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if datasets.is_empty() {
        return Err("no datasets provided for session save".into());
    }
    let dir = session_dir();
    fs::create_dir_all(&dir)?;
    let key = session_key_for_datasets(datasets);
    let session_path = dir.join(format!("{key}.json"));

    let state = SessionState::capture(app_state, active_file);
    let json = serde_json::to_string_pretty(&state)?;
    fs::write(&session_path, json)?;
    Ok(session_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use crate::data::slice::Bounds;
    use crate::render::colors::Palette;

    #[test]
    fn test_session_key_reproducibility() {
        let ds1 = vec!["data1.nc".to_string(), "data2.nc".to_string()];
        let ds2 = vec!["data1.nc".to_string(), "data2.nc".to_string()];
        let ds3 = vec!["data2.nc".to_string(), "data1.nc".to_string()];

        assert_eq!(
            session_key_for_datasets(&ds1),
            session_key_for_datasets(&ds2)
        );
        assert_ne!(
            session_key_for_datasets(&ds1),
            session_key_for_datasets(&ds3)
        );
    }

    #[test]
    fn test_session_state_capture_and_apply() {
        let mut app_state = AppState::default();
        app_state.view.selected_variable = Some("temperature".to_string());
        app_state.view.time_index = 5;
        app_state.view.depth_index = 2;
        app_state.view.palette = Palette::Plasma.toggle_reversed();
        app_state.view.scale_mode = ScaleMode::Log;
        app_state.view.color_scale_scope = ColorScaleScope::GlobalView;
        app_state.view.limits = Some((10.0, 100.0));
        app_state.view.limits_manual = true;
        app_state.view.filter_range = Some((15.0, 80.0));
        app_state.view.zoom_bounds = Some(Bounds::new(10, 50, 20, 60).unwrap());
        app_state.view.x_axis = Some("lon".to_string());
        app_state.view.y_axis = Some("lat".to_string());
        app_state.view.grid_mode = GridMode::Projected;
        app_state.view.show_land_borders = true;
        app_state.view.selected_point = Some((30, 40));
        app_state.view.selected_points = vec![(30, 40)];

        let captured = SessionState::capture(&app_state, 1);
        assert_eq!(captured.palette_name, "Plasma");
        assert!(captured.palette_reversed);
        assert_eq!(captured.time_index, 5);
        assert_eq!(captured.active_file, 1);

        let mut restored_state = AppState::default();
        let catalog = vec![Palette::Viridis, Palette::Plasma];
        let restored_active_file = captured.apply_to(&mut restored_state, &catalog);

        assert_eq!(restored_active_file, 1);
        assert_eq!(
            restored_state.view.selected_variable.as_deref(),
            Some("temperature")
        );
        assert_eq!(restored_state.view.time_index, 5);
        assert_eq!(restored_state.view.depth_index, 2);
        assert_eq!(restored_state.view.palette.name(), "Plasma");
        assert!(restored_state.view.palette.is_reversed());
        assert_eq!(restored_state.view.scale_mode, ScaleMode::Log);
        assert_eq!(
            restored_state.view.color_scale_scope,
            ColorScaleScope::GlobalView
        );
        assert_eq!(restored_state.view.limits, Some((10.0, 100.0)));
        assert!(restored_state.view.limits_manual);
        assert_eq!(restored_state.view.filter_range, Some((15.0, 80.0)));
        assert_eq!(
            restored_state.view.zoom_bounds,
            Some(Bounds::new(10, 50, 20, 60).unwrap())
        );
        assert_eq!(restored_state.view.x_axis.as_deref(), Some("lon"));
        assert_eq!(restored_state.view.y_axis.as_deref(), Some("lat"));
        assert_eq!(restored_state.view.grid_mode, GridMode::Projected);
        assert!(restored_state.view.show_land_borders);
        assert_eq!(restored_state.view.selected_point, Some((30, 40)));
        assert_eq!(restored_state.view.selected_points, vec![(30, 40)]);
    }

    #[test]
    fn test_session_round_trip_json() {
        let mut app_state = AppState::default();
        app_state.view.selected_variable = Some("salinity".to_string());
        app_state.view.time_index = 12;

        let session = SessionState::capture(&app_state, 0);
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(session, deserialized);
    }
}
