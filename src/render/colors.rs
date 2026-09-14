use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use colorous::{
    CIVIDIS, COOL, CUBEHELIX, Gradient, INFERNO, MAGMA, PLASMA, SPECTRAL, TURBO, VIRIDIS, WARM,
};

use crate::data::slice::{Slice2D, Validity};

mod vendored {
    include!(concat!(env!("OUT_DIR"), "/vendored_colormaps.rs"));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleMode {
    Linear,
    Log,
}

impl ScaleMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Log => "log10",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Palette {
    Viridis,
    Plasma,
    Turbo,
    Inferno,
    Magma,
    Cividis,
    Cool,
    Warm,
    Cubehelix,
    Spectral,
    /// A palette loaded from a three-column `.ncmap` file.  The format is
    /// used by the scientific colour maps distributed for Ncview.
    Custom(Arc<ScientificColorMap>),
    /// A palette with its low/high ends exchanged.
    Reversed(Box<Palette>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScientificColorMap {
    pub name: String,
    pub colors: Vec<[u8; 3]>,
}

#[derive(Clone, Copy)]
pub enum PaletteSampler<'a> {
    Gradient {
        gradient: Gradient,
        reversed: bool,
    },
    Custom {
        map: &'a ScientificColorMap,
        reversed: bool,
    },
}

impl<'a> PaletteSampler<'a> {
    pub fn new(palette: &'a Palette) -> Self {
        let mut curr = palette;
        let mut reversed = false;
        while let Palette::Reversed(inner) = curr {
            reversed = !reversed;
            curr = inner;
        }
        if let Some(gradient) = curr.gradient() {
            Self::Gradient { gradient, reversed }
        } else if let Palette::Custom(map) = curr {
            Self::Custom { map, reversed }
        } else {
            unreachable!("all non-custom palettes have a gradient")
        }
    }

    #[inline]
    pub fn sample(&self, position: f64) -> [u8; 3] {
        let pos = match self {
            Self::Gradient { reversed, .. } | Self::Custom { reversed, .. } if *reversed => {
                1.0 - position
            }
            _ => position,
        }
        .clamp(0.0, 1.0);

        match self {
            Self::Gradient { gradient, .. } => {
                let color = gradient.eval_continuous(pos);
                [color.r, color.g, color.b]
            }
            Self::Custom { map, .. } => {
                if map.colors.is_empty() {
                    return [0, 0, 0];
                }
                let max_idx = map.colors.len().saturating_sub(1);
                let scaled = pos * (max_idx as f64);
                let lower = scaled.floor() as usize;
                let upper = scaled.ceil() as usize;
                if lower == upper {
                    return map.colors[lower];
                }
                let fraction = scaled - lower as f64;
                let a = map.colors[lower];
                let b = map.colors[upper];
                [
                    (a[0] as f64 + (b[0] as f64 - a[0] as f64) * fraction).round() as u8,
                    (a[1] as f64 + (b[1] as f64 - a[1] as f64) * fraction).round() as u8,
                    (a[2] as f64 + (b[2] as f64 - a[2] as f64) * fraction).round() as u8,
                ]
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct MapOverlayColors {
    pub(crate) ocean: [u8; 3],
    pub(crate) land: [u8; 3],
    pub(crate) grid: [u8; 4],
    pub(crate) coast: [u8; 4],
}

impl Palette {
    pub fn name(&self) -> &str {
        match self {
            Self::Viridis => "Viridis",
            Self::Plasma => "Plasma",
            Self::Turbo => "Turbo",
            Self::Inferno => "Inferno",
            Self::Magma => "Magma",
            Self::Cividis => "Cividis",
            Self::Cool => "Cool",
            Self::Warm => "Warm",
            Self::Cubehelix => "Cubehelix",
            Self::Spectral => "Spectral",
            Self::Custom(map) => &map.name,
            Self::Reversed(palette) => palette.name(),
        }
    }

    pub fn is_reversed(&self) -> bool {
        matches!(self, Self::Reversed(_))
    }

    pub fn toggle_reversed(self) -> Self {
        match self {
            Self::Reversed(palette) => *palette,
            palette => Self::Reversed(Box::new(palette)),
        }
    }
    fn gradient(&self) -> Option<Gradient> {
        match self {
            Self::Viridis => Some(VIRIDIS),
            Self::Plasma => Some(PLASMA),
            Self::Turbo => Some(TURBO),
            Self::Inferno => Some(INFERNO),
            Self::Magma => Some(MAGMA),
            Self::Cividis => Some(CIVIDIS),
            Self::Cool => Some(COOL),
            Self::Warm => Some(WARM),
            Self::Cubehelix => Some(CUBEHELIX),
            Self::Spectral => Some(SPECTRAL),
            Self::Custom(_) => None,
            Self::Reversed(_) => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Viridis => Self::Plasma,
            Self::Plasma => Self::Turbo,
            Self::Turbo => Self::Inferno,
            Self::Inferno => Self::Magma,
            Self::Magma => Self::Cividis,
            Self::Cividis => Self::Cool,
            Self::Cool => Self::Warm,
            Self::Warm => Self::Cubehelix,
            Self::Cubehelix => Self::Spectral,
            Self::Spectral => Self::Viridis,
            Self::Custom(_) => Self::Viridis,
            Self::Reversed(palette) => Self::Reversed(Box::new(palette.next())),
        }
    }

    pub fn sampler(&self) -> PaletteSampler<'_> {
        PaletteSampler::new(self)
    }

    pub(crate) fn sample(&self, position: f64) -> [u8; 3] {
        self.sampler().sample(position)
    }
}

#[derive(Clone, Copy)]
pub struct ColorMapper<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
    min: f64,
    inv_range: f64,
    scale: ScaleMode,
    raw_min: f64,
    raw_max: f64,
    lut: [[u8; 3]; 256],
}

impl<'a> ColorMapper<'a> {
    pub fn new(
        palette: &'a Palette,
        stats: crate::data::slice::Statistics,
        limits: Option<(f64, f64)>,
        scale: ScaleMode,
    ) -> Self {
        let sampler = palette.sampler();
        let (raw_min, raw_max) = limits.unwrap_or((stats.min, stats.max));
        let (min, max) = match scale {
            ScaleMode::Linear => (raw_min, raw_max),
            ScaleMode::Log => {
                if raw_min > 0.0 && raw_max > 0.0 {
                    (raw_min.log10(), raw_max.log10())
                } else {
                    (raw_min, raw_max)
                }
            }
        };
        let inv_range = if min.is_finite() && max.is_finite() && max > min {
            1.0 / (max - min)
        } else {
            0.0
        };

        let mut lut = [[0u8; 3]; 256];
        for (i, entry) in lut.iter_mut().enumerate() {
            let pos = i as f64 / 255.0;
            *entry = sampler.sample(pos);
        }

        Self {
            _phantom: std::marker::PhantomData,
            min,
            inv_range,
            scale,
            raw_min,
            raw_max,
            lut,
        }
    }

    #[inline]
    pub fn map_value(&self, value: f64) -> [u8; 3] {
        if self.scale == ScaleMode::Log {
            if value <= 0.0 || self.raw_min <= 0.0 || self.raw_max <= 0.0 {
                return [80, 80, 80];
            }
            let val = value.log10();
            let norm = normalize_fast(val, self.min, self.inv_range);
            let idx = (norm * 255.0).round() as usize;
            self.lut[idx.min(255)]
        } else {
            let norm = normalize_fast(value, self.min, self.inv_range);
            let idx = (norm * 255.0).round() as usize;
            self.lut[idx.min(255)]
        }
    }
}

#[inline]
pub fn normalize_fast(value: f64, min: f64, inv_range: f64) -> f64 {
    if !value.is_finite() || inv_range == 0.0 {
        return 0.5;
    }
    ((value - min) * inv_range).clamp(0.0, 1.0)
}

impl Palette {
    /// Pick map-overlay colors that contrast with the low end of the active
    /// scientific palette. The overlay is presentation-only; field colors
    /// and the palette itself are never modified.
    pub(crate) fn map_overlay_colors(&self) -> MapOverlayColors {
        let low = self.sample(0.0);
        let luminance =
            (0.2126 * f64::from(low[0]) + 0.7152 * f64::from(low[1]) + 0.0722 * f64::from(low[2]))
                / 255.0;
        if luminance < 0.48 {
            MapOverlayColors {
                ocean: [15, 22, 36],
                land: [232, 238, 244],
                grid: [224, 236, 250, 175],
                coast: [255, 255, 255, 225],
            }
        } else {
            MapOverlayColors {
                ocean: [238, 242, 246],
                land: [43, 53, 67],
                grid: [32, 43, 58, 175],
                coast: [8, 18, 30, 225],
            }
        }
    }
}

/// Return compiled scientific maps followed by valid `.ncmap` files found in
/// the current directory or in one of the conventional Ncview data directories.
/// Invalid files are ignored so an optional colour-map bundle can never make a
/// dataset fail to open.
pub fn discover_colormaps() -> Vec<Palette> {
    // These maps are compiled into `colorous`, so the baseline scientific
    // catalog works in a single binary with no external color-map directory.
    let mut palettes = vec![
        Palette::Viridis,
        Palette::Plasma,
        Palette::Turbo,
        Palette::Inferno,
        Palette::Magma,
        Palette::Cividis,
        Palette::Cool,
        Palette::Warm,
        Palette::Cubehelix,
        Palette::Spectral,
    ];
    let mut names = palettes
        .iter()
        .map(|palette| palette.name().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut directories = vec![PathBuf::from(".")];
    for variable in ["NCVIEW_COLORMAPS", "NCVIEW_LIB_DIR", "NCVIEWBASE"] {
        if let Some(value) = env::var_os(variable) {
            let path = PathBuf::from(value);
            directories.push(path.clone());
            if variable == "NCVIEWBASE" {
                directories.push(path.join("colormaps"));
                directories.push(path.join("share").join("ncview"));
                directories.push(path.join("share").join("ncview").join("colormaps"));
            }
        }
    }
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("ncmap") {
                continue;
            }
            let Some(palette) = load_ncmap(&path) else {
                continue;
            };
            if names.insert(palette.name().to_ascii_lowercase()) {
                palettes.push(palette);
            }
        }
    }
    for (name, contents) in vendored::VENDORED {
        if names.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        if let Some(palette) = parse_ncmap(name, contents)
            && names.insert(palette.name().to_ascii_lowercase())
        {
            palettes.push(palette);
        }
    }
    palettes
}

/// Parse an Ncview `.ncmap` file: one RGB triplet (0..255) per line.
pub fn load_ncmap(path: &Path) -> Option<Palette> {
    let name = path.file_stem()?.to_str()?.to_owned();
    let contents = fs::read_to_string(path).ok()?;
    parse_ncmap(&name, &contents)
}

fn parse_ncmap(name: &str, contents: &str) -> Option<Palette> {
    let mut colors = Vec::new();
    for line in contents.lines() {
        let values = line
            .split('#')
            .next()?
            .split_whitespace()
            .collect::<Vec<_>>();
        if values.is_empty() {
            continue;
        }
        if values.len() != 3 {
            return None;
        }
        let mut rgb = [0u8; 3];
        for (index, value) in values.iter().enumerate() {
            rgb[index] = value.parse::<u16>().ok()?.try_into().ok()?;
        }
        colors.push(rgb);
    }
    (colors.len() >= 2).then_some(Palette::Custom(Arc::new(ScientificColorMap {
        name: name.to_owned(),
        colors,
    })))
}

#[inline]
pub fn normalize(value: f64, min: f64, max: f64) -> f64 {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() || max <= min {
        return 0.5;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

pub fn color_for(slice: &Slice2D, row: usize, col: usize, palette: &Palette) -> [u8; 3] {
    color_for_with_limits(slice, row, col, palette, None)
}

pub fn color_for_with_limits(
    slice: &Slice2D,
    row: usize,
    col: usize,
    palette: &Palette,
    limits: Option<(f64, f64)>,
) -> [u8; 3] {
    color_for_with_limits_and_filter_and_scale(
        slice,
        row,
        col,
        palette,
        limits,
        None,
        ScaleMode::Linear,
    )
}

pub fn color_for_with_limits_and_filter(
    slice: &Slice2D,
    row: usize,
    col: usize,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
) -> [u8; 3] {
    color_for_with_limits_and_filter_and_scale(
        slice,
        row,
        col,
        palette,
        limits,
        filter,
        ScaleMode::Linear,
    )
}

pub fn color_for_with_limits_and_filter_and_scale(
    slice: &Slice2D,
    row: usize,
    col: usize,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    scale: ScaleMode,
) -> [u8; 3] {
    let stats = slice.statistics.unwrap_or(crate::data::slice::Statistics {
        min: 0.0,
        max: 1.0,
        mean: 0.5,
        finite_count: 0,
    });
    color_for_value_with_limits_and_filter_and_scale(
        slice.values[(row, col)],
        slice.validity[(row, col)],
        stats,
        palette,
        limits,
        filter,
        scale,
    )
}

pub fn color_for_value_with_limits_and_filter_and_scale(
    value: f64,
    validity: Validity,
    stats: crate::data::slice::Statistics,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    scale: ScaleMode,
) -> [u8; 3] {
    if validity != Validity::Finite || !value.is_finite() {
        return [80, 80, 80];
    }
    if filter.is_some_and(|(min, max)| value < min || value > max) {
        return [30, 30, 46];
    }
    let (min, max) = limits.unwrap_or((stats.min, stats.max));
    if scale == ScaleMode::Log && (value <= 0.0 || min <= 0.0 || max <= 0.0) {
        return [80, 80, 80];
    }
    let (value, min, max) = match scale {
        ScaleMode::Linear => (value, min, max),
        ScaleMode::Log => (value.log10(), min.log10(), max.log10()),
    };
    palette.sample(normalize(value, min, max))
}

pub fn legend(palette: Palette, min: f64, max: f64) -> [(String, [u8; 3]); 3] {
    legend_with_scale(palette, min, max, ScaleMode::Linear)
}

pub fn legend_with_scale(
    palette: Palette,
    min: f64,
    max: f64,
    scale: ScaleMode,
) -> [(String, [u8; 3]); 3] {
    let midpoint = if scale == ScaleMode::Log && min > 0.0 && max > 0.0 {
        (min * max).sqrt()
    } else {
        (min + max) / 2.0
    };
    [
        (format!("{min:.4}"), palette.sample(0.0)),
        (format!("{midpoint:.4}"), palette.sample(0.5)),
        (format!("{max:.4}"), palette.sample(1.0)),
    ]
}

pub fn colorbar(palette: &Palette, steps: usize) -> Vec<[u8; 3]> {
    let steps = steps.max(2);
    (0..steps)
        .map(|index| palette.sample(index as f64 / (steps - 1) as f64))
        .collect()
}
