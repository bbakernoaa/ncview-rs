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
#[ignore = "requires public ERA5 sample"]
fn era5_member_time_and_pressure_match_netcdf_c() {
    let path = std::env::var("ERA5_SAMPLE").expect("ERA5_SAMPLE is required");
    let source = data::open(Path::new(&path)).unwrap();
    let file = oxinetcdf::NcFile::open(Path::new(&path)).unwrap();
    eprintln!("Links: {:?}", file.h5().dataset_names());
    for name in ["time", "longitude", "isobaricInhPa", "t"] {
        eprintln!(
            "{name}: metadata {:?}, attributes {:?}",
            file.h5().dataset_metadata(name).map(|data| data.shape),
            file.h5()
                .attr_views(name)
                .map(|attrs| attrs.iter().map(|attr| attr.name().to_owned()).collect::<Vec<_>>())
        );
    }
    eprintln!("ERA5 dimensions: {:?}", source.metadata().dimensions);
    eprintln!(
        "ERA5 temperature: {:?}",
        source.metadata().variables.iter().find(|v| v.name == "t")
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
