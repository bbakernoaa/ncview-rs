use ncview_rs::data::grib2_catalog::TableCatalog;
use ncview_rs::data::grib2_types::{RawCodeReason, TableResolution};

#[test]
fn known_aerosol_category_and_unknown_codes_retain_table_context() {
    let catalog = TableCatalog;
    assert!(matches!(
        catalog.resolve("4.1", 13),
        TableResolution::Defined { .. }
    ));
    assert!(matches!(
        catalog.resolve("4.233", 255),
        TableResolution::Raw {
            reason: RawCodeReason::Missing,
            ..
        }
    ));
    assert!(catalog.table_ids().len() > 100);
    assert!(catalog.known_table("4.233"));
    assert!(catalog.known_table("4.2-0-13"));
    assert_eq!(
        catalog.table_url("4.233").as_deref(),
        Some("https://www.nco.ncep.noaa.gov/pmb/docs/grib2/grib2_doc/grib2_table4-233.shtml")
    );
    assert!(catalog.known_template("1.2"));
    assert!(catalog.known_section("8"));
    assert!(catalog.known_document_entry("appendix-c"));
    assert_eq!(
        catalog.template_url("1.2").as_deref(),
        Some("https://www.nco.ncep.noaa.gov/pmb/docs/grib2/grib2_doc/grib2_temp1-2.shtml")
    );
}
