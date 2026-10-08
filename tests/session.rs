use ncview_rs::app::{AppState, ColorScaleScope, GridMode, ScaleMode};
use ncview_rs::data::slice::Bounds;
use ncview_rs::render::colors::Palette;
use ncview_rs::storage::session::{load_session, save_session, session_key_for_datasets};
use tempfile::TempDir;

#[test]
fn test_session_file_key_and_persistence() {
    let temp_dir = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("NCVIEW_SESSION_DIR", temp_dir.path());
    }

    let datasets = vec![
        "synthetic_file_a.nc".to_string(),
        "synthetic_file_b.nc".to_string(),
    ];

    // Verify initially no session exists
    assert!(load_session(&datasets).is_none());

    // Build AppState
    let mut state = AppState::default();
    state.view.selected_variable = Some("sea_surface_temperature".to_string());
    state.view.time_index = 3;
    state.view.depth_index = 1;
    state.view.palette = Palette::Turbo;
    state.view.scale_mode = ScaleMode::Log;
    state.view.color_scale_scope = ColorScaleScope::GlobalView;
    state.view.limits = Some((273.15, 310.15));
    state.view.limits_manual = true;
    state.view.filter_range = Some((280.0, 305.0));
    state.view.zoom_bounds = Some(Bounds::new(5, 25, 10, 35).unwrap());
    state.view.x_axis = Some("longitude".to_string());
    state.view.y_axis = Some("latitude".to_string());
    state.view.grid_mode = GridMode::Projected;
    state.view.show_land_borders = true;
    state.view.selected_point = Some((12, 18));
    state.view.selected_points = vec![(12, 18)];

    let saved_path = save_session(&datasets, &state, 1).expect("session save should succeed");
    assert!(saved_path.exists());

    // Check size of session file is very small (< 1 KB)
    let metadata = std::fs::metadata(&saved_path).unwrap();
    assert!(
        metadata.len() < 1024,
        "session file size should be small, got {} bytes",
        metadata.len()
    );

    // Restore session
    let loaded = load_session(&datasets).expect("saved session should load");
    assert_eq!(
        loaded.selected_variable.as_deref(),
        Some("sea_surface_temperature")
    );
    assert_eq!(loaded.time_index, 3);
    assert_eq!(loaded.depth_index, 1);
    assert_eq!(loaded.active_file, 1);
    assert_eq!(loaded.palette_name, "Turbo");
    assert!(!loaded.palette_reversed);
    assert_eq!(loaded.limits, Some((273.15, 310.15)));
    assert!(loaded.limits_manual);
    assert_eq!(loaded.filter_range, Some((280.0, 305.0)));
    assert_eq!(loaded.zoom_bounds, Some((5, 25, 10, 35)));
    assert_eq!(loaded.x_axis.as_deref(), Some("longitude"));
    assert_eq!(loaded.y_axis.as_deref(), Some("latitude"));
    assert!(loaded.show_land_borders);
    assert_eq!(loaded.selected_point, Some((12, 18)));

    // Verify key differs for a different dataset list
    let other_datasets = vec!["synthetic_file_c.nc".to_string()];
    assert_ne!(
        session_key_for_datasets(&datasets),
        session_key_for_datasets(&other_datasets)
    );
    assert!(load_session(&other_datasets).is_none());
}
