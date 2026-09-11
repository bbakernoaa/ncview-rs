//! Versioned GRIB2 table lookup facade.
//!
//! The catalog deliberately distinguishes two concerns: table *coverage* and
//! entry resolution. `known_table` covers every table published by the pinned
//! NOAA/NCEP documentation baseline, while unresolved values remain typed raw
//! values instead of being silently assigned a misleading short name. This
//! lets newer WMO and centre-local values pass through safely until the next
//! snapshot import adds their definitions.

use super::grib2_types::{RawCodeReason, TableResolution};
use grib::codetables::{CodeTable4_1, CodeTable4_2, Lookup};

#[derive(Debug, Clone, Copy, Default)]
pub struct TableCatalog;

impl TableCatalog {
    pub const NOAA_BASELINE: &'static str = "NCEP WMO GRIB2 documentation 37.0.0 (2026-06-01)";

    pub fn known_table(&self, table: &str) -> bool {
        NOAA_TABLE_IDS.contains(&table) || table.starts_with("4.2-")
    }

    pub fn known_template(&self, template: &str) -> bool {
        NOAA_TEMPLATE_IDS.contains(&template)
    }

    pub fn known_section(&self, section: &str) -> bool {
        NOAA_SECTION_IDS.contains(&section)
    }

    pub fn known_document_entry(&self, entry: &str) -> bool {
        matches!(
            entry,
            "introduction" | "revision-history" | "appendix-a" | "appendix-b" | "appendix-c"
        ) || self.known_section(entry)
            || self.known_template(entry.strip_prefix("template-").unwrap_or_default())
            || self.known_table(entry.strip_prefix("table-").unwrap_or_default())
    }

    pub fn table_ids(&self) -> &'static [&'static str] {
        NOAA_TABLE_IDS
    }

    pub fn table_url(&self, table: &str) -> Option<String> {
        self.known_table(table).then(|| {
            let table_path = if table.starts_with("4.2-") {
                "4-2".to_owned()
            } else {
                table.replace('.', "-")
            };
            format!(
                "https://www.nco.ncep.noaa.gov/pmb/docs/grib2/grib2_doc/grib2_table{table_path}.shtml"
            )
        })
    }

    pub fn template_url(&self, template: &str) -> Option<String> {
        self.known_template(template).then(|| {
            let template_path = template.replace('.', "-");
            format!(
                "https://www.nco.ncep.noaa.gov/pmb/docs/grib2/grib2_doc/grib2_temp{template_path}.shtml"
            )
        })
    }

    pub fn resolve(&self, table: &str, code: u64) -> TableResolution {
        let label = match (table, code) {
            ("0.0", 0) => Some("Meteorological Products"),
            ("0.0", 1) => Some("Hydrological Products"),
            ("0.0", 2) => Some("Land Surface Products"),
            ("0.0", 10) => Some("Oceanographic Products"),
            ("4.1", 13) => Some("Aerosols"),
            ("4.1", 14) => Some("Trace Gases"),
            ("4.1", 20) => Some("Atmospheric Chemical Constituents"),
            ("4.233", 0) => Some("Ozone"),
            ("4.233", 1) => Some("Water Vapor"),
            ("4.233", 2) => Some("Methane"),
            ("4.233", 3) => Some("Carbon Dioxide"),
            ("4.233", 4) => Some("Carbon Monoxide"),
            ("4.233", 5) => Some("Nitrogen Dioxide"),
            ("4.233", 6) => Some("Nitrous Oxide"),
            ("4.233", 7) => Some("Formaldehyde"),
            ("4.233", 8) => Some("Sulfur Dioxide"),
            ("4.233", 9) => Some("Ammonia"),
            ("4.233", 10) => Some("Ammonium Cation"),
            ("4.233", 11) => Some("Nitrogen Monoxide"),
            ("4.233", 12) => Some("Atomic Oxygen"),
            ("4.233", 13) => Some("Nitrate Radical"),
            ("4.233", 14) => Some("Hydroperoxyl Radical"),
            ("4.233", 15) => Some("Dinitrogen Pentoxide"),
            ("4.233", 16) => Some("Nitrous Acid"),
            ("4.233", 17) => Some("Nitric Acid"),
            ("4.233", 18) => Some("Peroxynitric Acid"),
            ("4.233", 19) => Some("Hydrogen Peroxide"),
            ("4.233", 20) => Some("Dihydrogen"),
            ("4.233", 21) => Some("Atomic Nitrogen"),
            ("4.233", 22) => Some("Sulfate Anion"),
            ("4.233", 23) => Some("Atomic Radon"),
            ("4.233", 24) => Some("Mercury Vapor"),
            ("4.233", 25) => Some("Mercury(II) Cation"),
            ("4.233", 26) => Some("Atomic Chlorine"),
            ("4.233", 27) => Some("Chlorine Monoxide"),
            ("4.233", 28) => Some("Dichlorine Peroxide"),
            ("4.233", 29) => Some("Hypochlorous Acid"),
            ("4.233", 30) => Some("Chlorine Nitrate"),
            ("4.233", 31) => Some("Chlorine Dioxide"),
            ("4.233", 32) => Some("Atomic Bromine"),
            ("4.233", 33) => Some("Bromine Monoxide"),
            ("4.233", 34) => Some("Bromine Chloride"),
            ("4.233", 35) => Some("Hydrogen Bromide"),
            ("4.233", 36) => Some("Hypobromous Acid"),
            ("4.233", 37) => Some("Bromine Nitrate"),
            ("4.233", 38) => Some("Dioxygen"),
            ("4.233", 39) => Some("Nitryl Chloride"),
            ("4.233", 40) => Some("Sulfuric Acid"),
            ("4.233", 41) => Some("Hydrogen Sulfide"),
            ("4.233", 42) => Some("Sulfur Trioxide"),
            ("4.233", 43) => Some("Bromine"),
            ("4.233", 62000) => Some("Total Aerosol"),
            ("4.233", 62001) => Some("Dust Dry"),
            ("4.233", 62002) => Some("Water in Ambient"),
            ("4.233", 62003) => Some("Ammonium Dry"),
            ("4.233", 62004) => Some("Nitrate Dry"),
            ("4.233", 62005) => Some("Nitric Acid Trihydrate"),
            ("4.233", 62006) => Some("Sulfate Dry"),
            ("4.233", 62007) => Some("Mercury Dry"),
            ("4.233", 62008) => Some("Sea Salt Dry"),
            ("4.233", 62009) => Some("Black Carbon Dry"),
            ("4.233", 62010) => Some("Particulate Organic Matter Dry"),
            ("4.233", 62011) => Some("Primary Particulate Organic Matter Dry"),
            ("4.233", 62012) => Some("Secondary Particulate Organic Matter Dry"),
            ("4.233", 62013) => Some("Black Carbon Hydrophilic Dry"),
            ("4.233", 62014) => Some("Black Carbon Hydrophobic Dry"),
            ("4.233", 62015) => Some("Particulate Organic Matter Hydrophilic Dry"),
            ("4.233", 62016) => Some("Particulate Organic Matter Hydrophobic Dry"),
            ("4.233", 62017) => Some("Nitrate Hydrophilic Dry"),
            ("4.233", 62018) => Some("Nitrate Hydrophobic Dry"),
            ("4.233", 62020) => Some("Smoke High Absorption"),
            ("4.233", 62021) => Some("Smoke Low Absorption"),
            ("4.233", 62022) => Some("Aerosol High Absorption"),
            ("4.233", 62023) => Some("Aerosol Low Absorption"),
            ("4.233", 62025) => Some("Volcanic Ash"),
            ("4.233", 62026) => Some("Particulate Matter"),
            ("4.233", 62028) => Some("Total Aerosol Hydrophilic"),
            ("4.233", 62029) => Some("Total Aerosol Hydrophobic"),
            ("4.233", 62030) => Some("Primary Particulate Inorganic Matter Dry"),
            ("4.233", 62031) => Some("Secondary Particulate Inorganic Matter Dry"),
            ("4.233", 62032) => Some("Biogenic Secondary Organic Aerosol"),
            ("4.233", 62033) => Some("Anthropogenic Secondary Organic Aerosol"),
            ("4.233", 62034) => Some("Rain Water"),
            ("4.233", 62035) => Some("Cloud Water"),
            ("4.233", 62036) => Some("Brown Carbon Dry"),
            ("4.233", 62037) => Some("Sea Salt Wet at 80% Relative Humidity"),
            _ => None,
        };
        match label {
            Some(label) => TableResolution::Defined {
                table: table.into(),
                code,
                label: label.into(),
                unit: None,
            },
            None => TableResolution::Raw {
                table: table.into(),
                code,
                reason: if code == 255 {
                    RawCodeReason::Missing
                } else if code >= 192 {
                    RawCodeReason::LocalUse
                } else {
                    RawCodeReason::Unknown
                },
            },
        }
    }

    /// Resolve the discipline/category-dependent parameter tables using the
    /// WMO/NOAA lookup data shipped by the decoder, while preserving the
    /// discipline and category in the table identity.
    pub fn resolve_parameter(
        &self,
        discipline: u8,
        category: u8,
        number: u8,
    ) -> (TableResolution, TableResolution) {
        let category_text = CodeTable4_1::new(discipline)
            .lookup(usize::from(category))
            .to_string();
        let parameter_text = CodeTable4_2::new(discipline, category)
            .lookup(usize::from(number))
            .to_string();
        (
            self.resolution_from_lookup("4.1", u64::from(category), category_text),
            self.resolution_from_lookup(
                &format!("4.2-{discipline}-{category}"),
                u64::from(number),
                parameter_text,
            ),
        )
    }

    pub fn parameter_label(&self, discipline: u8, category: u8, number: u8) -> Option<String> {
        if let Some(label) = explicit_parameter_label(discipline, category, number) {
            return Some(label.into());
        }
        let label = CodeTable4_2::new(discipline, category)
            .lookup(usize::from(number))
            .to_string();
        if label.contains("not implemented") {
            if discipline == 0 && category == 13 && number >= 192 {
                Some(format!("Aerosol local parameter {number}"))
            } else {
                Some(format!(
                    "{} parameter {number} (local or newer table entry)",
                    category_label(discipline, category)
                ))
            }
        } else {
            Some(label)
        }
    }

    fn resolution_from_lookup(&self, table: &str, code: u64, label: String) -> TableResolution {
        if label.contains("not implemented") {
            self.resolve(table, code)
        } else {
            TableResolution::Defined {
                table: table.into(),
                code,
                label,
                unit: None,
            }
        }
    }
}

fn explicit_parameter_label(discipline: u8, category: u8, number: u8) -> Option<&'static str> {
    match (discipline, category, number) {
        (0, 13, 0) => Some("Aerosol Type"),
        (0, 20, 100) => Some("Surface Area Density (Aerosol)"),
        (0, 20, 101) => Some("Vertical Visual Range"),
        (0, 20, 102) => Some("Aerosol Optical Thickness"),
        (0, 20, 103) => Some("Single Scattering Albedo"),
        (0, 20, 104) => Some("Asymmetry Factor"),
        (0, 20, 105) => Some("Aerosol Extinction Coefficient"),
        (0, 20, 106) => Some("Aerosol Absorption Coefficient"),
        (0, 20, 107) => Some("Aerosol Lidar Backscatter from Satellite"),
        (0, 20, 108) => Some("Aerosol Lidar Backscatter from the Ground"),
        (0, 20, 109) => Some("Aerosol Lidar Extinction from Satellite"),
        (0, 20, 110) => Some("Aerosol Lidar Extinction from the Ground"),
        (0, 20, 111) => Some("Angstrom Exponent"),
        (0, 20, 112) => Some("Absorption Aerosol Optical Thickness"),
        (0, 20, 113) => Some("Aerosol Backscatter Coefficient"),
        _ => None,
    }
}

fn category_label(discipline: u8, category: u8) -> &'static str {
    match (discipline, category) {
        (0, 13) => "Aerosol",
        (0, 20) => "Atmospheric chemical constituent",
        _ => "GRIB2",
    }
}

/// Table identifiers linked by the NCEP/WMO baseline. The list intentionally
/// includes tables that do not have a compact fixed-width entry model (for
/// example template, flag, and parameter tables); their raw values are still
/// retained by the resolver.
const NOAA_TABLE_IDS: &[&str] = &[
    "0.0", "1.0", "1.1", "1.2", "1.3", "1.4", "1.5", "1.6", "3.0", "3.1", "3.2", "3.3", "3.4",
    "3.5", "3.6", "3.7", "3.8", "3.9", "3.10", "3.11", "3.12", "3.13", "3.15", "3.20", "3.21",
    "3.25", "4.0", "4.1", "4.2", "4.3", "4.4", "4.5", "4.6", "4.7", "4.8", "4.9", "4.10", "4.11",
    "4.12", "4.13", "4.14", "4.15", "4.16", "4.91", "4.100", "4.101", "4.102", "4.103", "4.104",
    "4.105", "4.106", "4.120", "4.121", "4.122", "4.201", "4.202", "4.203", "4.204", "4.205",
    "4.206", "4.207", "4.208", "4.209", "4.210", "4.211", "4.212", "4.213", "4.214", "4.215",
    "4.216", "4.217", "4.218", "4.219", "4.220", "4.221", "4.222", "4.223", "4.224", "4.225",
    "4.227", "4.228", "4.230", "4.233", "4.234", "4.236", "4.238", "4.239", "4.240", "4.241",
    "4.242", "4.243", "4.244", "4.246", "4.247", "4.248", "4.249", "4.250", "4.251", "4.252",
    "4.253", "4.254", "4.333", "4.335", "4.336", "5.0", "5.1", "5.2", "5.3", "5.4", "5.5", "5.6",
    "5.7", "5.25", "5.26", "5.40", "6.0", "7.0",
];

const NOAA_TEMPLATE_IDS: &[&str] = &["1.0", "1.1", "1.2"];

const NOAA_SECTION_IDS: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defined_entries_and_raw_fallback_are_distinct() {
        let catalog = TableCatalog;
        assert!(matches!(
            catalog.resolve("4.1", 13),
            TableResolution::Defined { .. }
        ));
        assert!(matches!(
            catalog.resolve("4.1", 255),
            TableResolution::Raw {
                reason: RawCodeReason::Missing,
                ..
            }
        ));
        assert!(catalog.known_table("4.336"));
        assert!(!catalog.known_table("9.9"));
        assert!(catalog.known_template("1.2"));
        assert!(catalog.known_section("8"));
        assert!(catalog.known_document_entry("appendix-c"));
    }
}
