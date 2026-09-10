#[cfg(test)]
mod tests {
    use super::super::colors::{
        Palette, ScaleMode, discover_colormaps, legend_with_scale, load_ncmap, normalize,
    };

    #[test]
    fn normalization_clamps_and_handles_constant_slices() {
        assert_eq!(normalize(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(normalize(2.0, 0.0, 1.0), 1.0);
        assert_eq!(normalize(1.0, 1.0, 1.0), 0.5);
    }

    #[test]
    fn reversed_palette_swaps_low_and_high_ends() {
        let palette = Palette::Viridis;
        let reversed = palette.clone().toggle_reversed();
        assert!(reversed.is_reversed());
        assert_eq!(reversed.sample(0.0), palette.sample(1.0));
        assert_eq!(reversed.sample(1.0), palette.sample(0.0));
        assert_eq!(reversed.toggle_reversed(), palette);
    }

    #[test]
    fn parses_ncview_scientific_colour_map_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("batlow.ncmap");
        std::fs::write(&path, "0 10 20\n  100 110 120\n255 245 235\n").unwrap();
        let palette = load_ncmap(&path).expect("valid ncmap");
        assert_eq!(palette.name(), "batlow");
        assert_eq!(palette.sample(0.0), [0, 10, 20]);
        assert_eq!(palette.sample(1.0), [255, 245, 235]);
        assert!(matches!(palette, Palette::Custom(_)));
    }

    #[test]
    fn rejects_malformed_or_out_of_range_ncmap_files() {
        let directory = tempfile::tempdir().unwrap();
        let malformed = directory.path().join("bad.ncmap");
        std::fs::write(&malformed, "0 0\n").unwrap();
        assert!(load_ncmap(&malformed).is_none());
        let out_of_range = directory.path().join("range.ncmap");
        std::fs::write(&out_of_range, "0 0 0\n256 0 0\n").unwrap();
        assert!(load_ncmap(&out_of_range).is_none());
    }

    #[test]
    fn logarithmic_legend_uses_geometric_midpoint() {
        let labels = legend_with_scale(Palette::Viridis, 1.0, 100.0, ScaleMode::Log);
        assert_eq!(labels[1].0, "10.0000");
    }

    #[test]
    fn includes_vendored_scientific_colour_maps() {
        let names = discover_colormaps()
            .into_iter()
            .map(|palette| palette.name().to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>();
        assert!(names.contains("batlow"));
        assert!(names.contains("vik"));
        assert!(names.contains("roma"));
    }
}
