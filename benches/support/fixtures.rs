#![allow(dead_code)] // Shared builders are consumed by multiple benchmark targets.

use std::sync::Arc;

use ncview_rs::{
    data::{DatasetFormat, DatasetMetadata, Variable, fixtures::regular_values},
    render::colors::{Palette, ScientificColorMap},
};

pub fn large_regular_slice() -> ncview_rs::data::slice::Slice2D {
    regular_values(2048, 1024).expect("benchmark field dimensions are valid")
}

pub fn variable_catalog(count: usize) -> Vec<Variable> {
    (0..count)
        .map(|index| Variable {
            name: format!("variable_{index:05}"),
            dimensions: vec!["latitude".into(), "longitude".into()],
            numeric: true,
            units: None,
            long_name: None,
            standard_name: None,
        })
        .collect()
}

pub fn palette_catalog(count: usize) -> Vec<Palette> {
    (0..count)
        .map(|index| {
            Palette::Custom(Arc::new(ScientificColorMap {
                name: format!("Benchmark{index:03}"),
                colors: vec![[10, 20, 30], [120, 140, 160], [240, 230, 220]],
            }))
        })
        .collect()
}

pub fn metadata_with_variables(count: usize) -> DatasetMetadata {
    DatasetMetadata {
        path: "benchmark".into(),
        format: DatasetFormat::NetCdf4,
        dimensions: Vec::new(),
        variables: variable_catalog(count),
    }
}
