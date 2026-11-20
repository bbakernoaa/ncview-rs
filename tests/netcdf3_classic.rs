use ncview_rs::data::{
    self,
    slice::{Bounds, SliceRequest, Validity},
};

#[test]
fn netcdf3_time_slice_preserves_packing_and_missing_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.nc");
    // Classic NetCDF with temp(time=2, lat=2, lon=3), packed as i16.
    let hex = concat!(
        "43444601000000000000000a000000030000000474696d6500000002000000036c6174000000000200000003",
        "6c6f6e000000000300000000000000000000000b000000010000000474656d70000000030000000000000001",
        "000000020000000c000000030000000a5f46696c6c56616c756500000000000300000001fc1900000000000c",
        "7363616c655f666163746f7200000005000000013f0000000000000a6164645f6f6666736574000000000005",
        "00000001412000000000000300000018000000c400000002000400060008000a000c000e00100012fc190016",
    );
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    std::fs::write(&path, bytes).unwrap();
    let source = data::open(&path).unwrap();
    let slice = source
        .read_slice(&SliceRequest {
            variable: "temp".into(),
            time: 1,
            depth: 0,
            bounds: Bounds::new(0, 2, 1, 3).unwrap(),
        })
        .unwrap();
    assert_eq!(slice.values[(0, 0)], 17.0);
    assert_eq!(slice.values[(0, 1)], 18.0);
    assert_eq!(slice.validity[(1, 0)], Validity::Fill);
    assert_eq!(slice.values[(1, 1)], 21.0);
}
