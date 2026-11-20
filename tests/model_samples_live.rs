//! Optional integration checks against public, pinned scientific model samples.
//! Reference NetCDF values were read using the netCDF-C library.

use ncview_rs::data::{
    self, AxisRole,
    slice::{Bounds, SliceRequest, Validity},
};
use std::path::Path;

fn assert_values(slice: &data::slice::Slice2D, expected: &[f64]) {
    assert_eq!(slice.values.len(), expected.len());
    for ((value, validity), expected) in
        slice.values.iter().zip(slice.validity.iter()).zip(expected)
    {
        assert_eq!(*validity, Validity::Finite);
        assert!((value - expected).abs() < 1.0e-5, "{value} != {expected}");
    }
}

#[test]
#[ignore = "requires public MPAS-A diagnostic sample"]
fn mpas_atmosphere_diagnostics_match_netcdf_c() {
    let path = std::env::var("MPAS_DIAG_SAMPLE").expect("MPAS_DIAG_SAMPLE is required");
    let source = data::open(Path::new(&path)).expect("open the MPAS-A diagnostic file");
    let metadata = source.metadata();
    assert!(metadata.dimensions.iter().any(|dimension| {
        dimension.name == "nIsoLevelsT"
            && dimension.length == 5
            && dimension.role == AxisRole::Depth
    }));
    assert_eq!(
        source.time_label(0).as_deref(),
        Some("2023-10-26T13:00:00Z")
    );

    // netCDF-C read the fields and cell coordinates. A brute-force spherical
    // nearest-neighbor search selected the six cells in this quarter-degree window.
    let bounds = Bounds::new(515, 517, 339, 342).unwrap();
    for (variable, depth, expected) in [
        (
            "t2m",
            0,
            [
                291.995_419_456_440_35,
                291.889_895_102_234_4,
                292.150_448_266_915_8,
                292.031_013_430_886_7,
                292.126_775_298_090_9,
                292.489_749_760_049_6,
            ],
        ),
        (
            "t_isobaric",
            0,
            [
                237.716_452_371_126_2,
                238.634_002_616_694_12,
                238.724_723_171_934_47,
                237.648_575_061_598_65,
                238.245_963_663_713_92,
                238.397_651_282_714_83,
            ],
        ),
        (
            "t_isobaric",
            2,
            [
                254.537_340_852_408_02,
                254.526_130_510_491_07,
                254.048_167_111_749_56,
                254.510_905_209_516_58,
                254.290_344_752_725_8,
                253.918_667_552_310_86,
            ],
        ),
    ] {
        let slice = source
            .read_slice(&SliceRequest {
                variable: variable.into(),
                time: 0,
                depth,
                bounds,
            })
            .unwrap_or_else(|error| panic!("read {variable} at depth {depth}: {error}"));
        assert_eq!(slice.values.dim(), (2, 3));
        assert_values(&slice, &expected);
    }
}

#[test]
#[ignore = "requires public ERA5 sample"]
fn era5_member_time_and_pressure_match_netcdf_c() {
    let path = std::env::var("ERA5_SAMPLE").expect("ERA5_SAMPLE is required");
    let source = data::open(Path::new(&path)).unwrap();
    let temperature = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "t")
        .unwrap();
    assert_eq!(
        temperature.dimensions,
        ["number", "time", "isobaricInhPa", "latitude", "longitude"]
    );
    let slice = source
        .read_slice_on_axes(
            &SliceRequest {
                variable: "t".into(),
                time: 1,
                depth: 1,
                bounds: Bounds::new(0, 2, 0, 3).unwrap(),
            },
            Some("latitude"),
            Some("longitude"),
            &[("number".into(), 2)],
        )
        .unwrap();
    assert_values(
        &slice,
        &[
            233.132_324_218_75,
            233.132_324_218_75,
            233.132_324_218_75,
            231.145_019_531_25,
            231.078_613_281_25,
            231.019_042_968_75,
        ],
    );
}

#[test]
#[ignore = "requires public WRF sample"]
fn wrf_mass_and_staggered_wind_slices_match_netcdf_c() {
    let path = std::env::var("WRF_SAMPLE").expect("WRF_SAMPLE is required");
    let source = data::open(Path::new(&path)).unwrap();
    for (variable, x_axis, expected) in [
        (
            "T",
            "west_east",
            [
                2.154_663_085_937_5,
                2.111_511_230_468_75,
                2.078_033_447_265_625,
                2.096_527_099_609_375,
                2.047_973_632_812_5,
                2.019_561_767_578_125,
            ],
        ),
        (
            "U",
            "west_east_stag",
            [
                -3.348_489_999_771_118,
                -3.299_271_345_138_55,
                -3.217_454_671_859_741,
                -3.400_893_926_620_483_4,
                -3.323_473_691_940_307_6,
                -3.241_217_374_801_635_7,
            ],
        ),
    ] {
        let slice = source
            .read_slice_on_axes(
                &SliceRequest {
                    variable: variable.into(),
                    time: 0,
                    depth: 1,
                    bounds: Bounds::new(0, 2, 0, 3).unwrap(),
                },
                Some("south_north"),
                Some(x_axis),
                &[],
            )
            .unwrap();
        assert_values(&slice, &expected);
    }
}

#[test]
#[ignore = "requires NCEP NAM GRIB2 message and ecCodes reference"]
fn ncep_nam_grib2_range_matches_eccodes() {
    let path = std::env::var("NAM_SAMPLE").expect("NAM_SAMPLE is required");
    let expected =
        std::fs::read_to_string(std::env::var("NAM_REFERENCE").expect("NAM_REFERENCE is required"))
            .unwrap();
    let reference = expected
        .split_whitespace()
        .map(|value| value.parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reference.len(), 2);
    let source = data::open(Path::new(&path)).unwrap();
    let metadata = source.metadata();
    let variable = metadata.variables.iter().find(|item| item.numeric).unwrap();
    let length = |role| {
        metadata
            .dimensions
            .iter()
            .find(|dim| dim.role == role)
            .unwrap()
            .length
    };
    let slice = source
        .read_slice(&SliceRequest {
            variable: variable.name.clone(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(
                0,
                length(AxisRole::Latitude),
                0,
                length(AxisRole::Longitude),
            )
            .unwrap(),
        })
        .unwrap();
    let stats = slice.statistics.unwrap();
    // The GRIB decoder performs unpacking in f32. Permit one f32 epsilon at
    // the magnitude of each independently decoded ecCodes extremum.
    let tolerance = |value: f64| f64::from(f32::EPSILON) * value.abs().max(1.0);
    assert!(
        (stats.min - reference[0]).abs() <= tolerance(reference[0]),
        "ncv min {} != ecCodes {}",
        stats.min,
        reference[0]
    );
    assert!(
        (stats.max - reference[1]).abs() <= tolerance(reference[1]),
        "ncv max {} != ecCodes {}",
        stats.max,
        reference[1]
    );
}
