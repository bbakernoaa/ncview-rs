use ncview_rs::data::grib2_identity::{AerosolQualifierSet, FieldIdentity};
use ncview_rs::data::grib2_types::{
    Grib2MessageHeader, Grib2SourceLocation, RawCodeReason, TableResolution,
};

fn header() -> Grib2MessageHeader {
    Grib2MessageHeader {
        location: Grib2SourceLocation {
            path: "fixture.grib2".into(),
            message: 0,
            offset: Some(0),
        },
        discipline: 0,
        centre: 7,
        subcentre: 0,
        master_table_version: 37,
        local_table_version: 1,
        grid_template: 0,
        product_template: 44,
        data_representation_template: 0,
        grid_points: 4,
        product_definition_payload: vec![0, 0, 0, 0, 13, 193],
        parameter_category: Some(13),
        parameter_number: Some(193),
        description: "AOTK aerosol".into(),
        resolution: vec![TableResolution::Raw {
            table: "4.2".into(),
            code: 193,
            reason: RawCodeReason::Unknown,
        }],
    }
}

#[test]
fn repeated_short_names_have_distinct_species_and_size_keys() {
    let base = FieldIdentity::from_header(&header());
    let dust = base.clone().with_aerosol_qualifiers(AerosolQualifierSet {
        species: Some("dust".into()),
        first_size: Some("0.1m".into()),
        ..AerosolQualifierSet::default()
    });
    let sulfate =
        FieldIdentity::from_header(&header()).with_aerosol_qualifiers(AerosolQualifierSet {
            species: Some("sulfate".into()),
            first_size: Some("1.0m".into()),
            ..AerosolQualifierSet::default()
        });
    assert_eq!(base.short_name, "aotk");
    assert_ne!(dust.machine_key, sulfate.machine_key);
}

#[test]
fn optical_aerosol_templates_include_species_sizes_and_wavelengths() {
    let mut product = vec![0; 30];
    product[2..4].copy_from_slice(&48_u16.to_be_bytes());
    product[4] = 13;
    product[5] = 193;
    product[6..8].copy_from_slice(&22_u16.to_be_bytes());
    product[8] = 1;
    product[9] = 2;
    product[10..14].copy_from_slice(&10_i32.to_be_bytes());
    product[14] = 2;
    product[15..19].copy_from_slice(&20_i32.to_be_bytes());
    product[19] = 1;
    product[20] = 0;
    product[21..25].copy_from_slice(&550_i32.to_be_bytes());
    product[25] = 0;
    product[26..30].copy_from_slice(&600_i32.to_be_bytes());
    let mut header = header();
    header.product_template = 48;
    header.product_definition_payload = product;
    header.resolution.push(TableResolution::Defined {
        table: "4.233".into(),
        code: 22,
        label: "Sulfate Anion".into(),
        unit: None,
    });
    let identity = FieldIdentity::from_header(&header);
    assert!(identity.machine_key.contains("species=Sulfate Anion"));
    assert!(identity.machine_key.contains("wavelength1="));
    assert!(identity.machine_key.contains("wavelength2="));
}
