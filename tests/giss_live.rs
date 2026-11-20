use ncview_rs::data::{
    self,
    slice::{Bounds, SliceRequest, Validity},
};

#[test]
#[ignore = "requires NASA ModelE sample and independent reference values"]
fn public_giss_sample_matches_reference_values_and_mask() {
    let path = std::env::var("GISS_SAMPLE").expect("GISS_SAMPLE is required");
    let expected = std::fs::read_to_string(
        std::env::var("GISS_REFERENCE").expect("GISS_REFERENCE is required"),
    )
    .unwrap();
    let source = data::open(std::path::Path::new(&path)).unwrap();
    let slice = source
        .read_slice(&SliceRequest {
            variable: "FRMP".into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(0, 90, 0, 144).unwrap(),
        })
        .unwrap();
    let mut missing = 0;
    let mut finite = 0;
    for (index, line) in expected.lines().enumerate() {
        let (value, mask) = line.split_once(' ').unwrap();
        let row = index / 144;
        let col = index % 144;
        if mask == "1" {
            assert_eq!(slice.validity[(row, col)], Validity::Missing);
            missing += 1;
        } else {
            assert_eq!(slice.validity[(row, col)], Validity::Finite);
            assert_eq!(slice.values[(row, col)], value.parse::<f64>().unwrap());
            finite += 1;
        }
    }
    assert_eq!(missing + finite, 90 * 144);
    assert!(missing > 0 && finite > 0);
}
