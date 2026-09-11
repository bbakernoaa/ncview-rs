use ncview_rs::data::grib2_types::{
    Grib2Diagnostic, Grib2MessageHeader, Grib2SourceLocation, RawCodeReason, TableResolution,
};

#[test]
fn source_and_resolution_metadata_retain_raw_codes() {
    let header = Grib2MessageHeader {
        location: Grib2SourceLocation {
            path: "fixture.grib2".into(),
            message: 3,
            offset: Some(128),
        },
        discipline: 0,
        centre: 7,
        subcentre: 0,
        master_table_version: 37,
        local_table_version: 1,
        grid_template: 0,
        product_template: 48,
        data_representation_template: 0,
        grid_points: 4,
        product_definition_payload: vec![0, 0, 0, 0, 13, 193, 48],
        parameter_category: Some(13),
        parameter_number: Some(193),
        description: "AOTK aerosol optical depth".into(),
        resolution: vec![TableResolution::Raw {
            table: "4.2-0-13".into(),
            code: 193,
            reason: RawCodeReason::LocalUse,
        }],
    };
    assert_eq!(header.location.message, 3);
    assert_eq!(header.raw_code_label(), "d0-c13-p193-g0-t48-r0");
    assert!(matches!(
        header.resolution[0],
        TableResolution::Raw {
            reason: RawCodeReason::LocalUse,
            ..
        }
    ));
}

#[test]
fn diagnostics_have_stable_machine_codes() {
    let diagnostic = Grib2Diagnostic::new("GRIB2_IDX_RANGE", None, "invalid range");
    assert_eq!(diagnostic.code, "GRIB2_IDX_RANGE");
    assert_eq!(diagnostic.message, "invalid range");
}
