use std::path::Path;

use ncview_rs::data::grib2_index::parse_index;

#[test]
fn index_ranges_are_source_bounded_and_deterministic() {
    let records = parse_index(
        Path::new("fixture.grib2.idx"),
        "1:0:d=2026091100:TMP:surface\n2:25:d=2026091100:RH:surface\n",
        30,
    )
    .unwrap();
    assert_eq!(records[0].length, 25);
    assert_eq!(records[1].length, 5);
    assert_eq!(records[0].fields[3], "TMP");
}

#[test]
fn invalid_index_never_produces_a_reference_range() {
    let error = parse_index(
        Path::new("fixture.grib2.idx"),
        "1:31:d=2026091100:TMP:surface\n",
        30,
    )
    .unwrap_err();
    assert!(error.to_string().contains("outside the source object"));
}
