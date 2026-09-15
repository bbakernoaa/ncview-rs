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
        match Self::fixed_label(table, code) {
            Some(label) => TableResolution::Defined {
                table: table.into(),
                code,
                label: label.into(),
                unit: None,
            },
            None => TableResolution::Raw {
                table: table.into(),
                code,
                reason: Self::raw_code_reason(code),
            },
        }
    }

    fn fixed_label(table: &str, code: u64) -> Option<&'static str> {
        match table {
            "0.0" => match code {
                0 => Some("Meteorological Products"),
                1 => Some("Hydrological Products"),
                2 => Some("Land Surface Products"),
                10 => Some("Oceanographic Products"),
                _ => None,
            },
            "4.1" => match code {
                13 => Some("Aerosols"),
                14 => Some("Trace Gases"),
                20 => Some("Atmospheric Chemical Constituents"),
                _ => None,
            },
            "4.233" => Self::AEROSOL_LABELS
                .binary_search_by_key(&code, |(known_code, _)| *known_code)
                .ok()
                .map(|index| Self::AEROSOL_LABELS[index].1),
            _ => None,
        }
    }

    fn raw_code_reason(code: u64) -> RawCodeReason {
        if code == 255 {
            RawCodeReason::Missing
        } else if code >= 192 {
            RawCodeReason::LocalUse
        } else {
            RawCodeReason::Unknown
        }
    }

    const AEROSOL_LABELS: &[(u64, &str)] = &[
        (0, "Ozone"),
        (1, "Water Vapor"),
        (2, "Methane"),
        (3, "Carbon Dioxide"),
        (4, "Carbon Monoxide"),
        (5, "Nitrogen Dioxide"),
        (6, "Nitrous Oxide"),
        (7, "Formaldehyde"),
        (8, "Sulfur Dioxide"),
        (9, "Ammonia"),
        (10, "Ammonium Cation"),
        (11, "Nitrogen Monoxide"),
        (12, "Atomic Oxygen"),
        (13, "Nitrate Radical"),
        (14, "Hydroperoxyl Radical"),
        (15, "Dinitrogen Pentoxide"),
        (16, "Nitrous Acid"),
        (17, "Nitric Acid"),
        (18, "Peroxynitric Acid"),
        (19, "Hydrogen Peroxide"),
        (20, "Dihydrogen"),
        (21, "Atomic Nitrogen"),
        (22, "Sulfate Anion"),
        (23, "Atomic Radon"),
        (24, "Mercury Vapor"),
        (25, "Mercury(II) Cation"),
        (26, "Atomic Chlorine"),
        (27, "Chlorine Monoxide"),
        (28, "Dichlorine Peroxide"),
        (29, "Hypochlorous Acid"),
        (30, "Chlorine Nitrate"),
        (31, "Chlorine Dioxide"),
        (32, "Atomic Bromine"),
        (33, "Bromine Monoxide"),
        (34, "Bromine Chloride"),
        (35, "Hydrogen Bromide"),
        (36, "Hypobromous Acid"),
        (37, "Bromine Nitrate"),
        (38, "Dioxygen"),
        (39, "Nitryl Chloride"),
        (40, "Sulfuric Acid"),
        (41, "Hydrogen Sulfide"),
        (42, "Sulfur Trioxide"),
        (43, "Bromine"),
        (62000, "Total Aerosol"),
        (62001, "Dust Dry"),
        (62002, "Water in Ambient"),
        (62003, "Ammonium Dry"),
        (62004, "Nitrate Dry"),
        (62005, "Nitric Acid Trihydrate"),
        (62006, "Sulfate Dry"),
        (62007, "Mercury Dry"),
        (62008, "Sea Salt Dry"),
        (62009, "Black Carbon Dry"),
        (62010, "Particulate Organic Matter Dry"),
        (62011, "Primary Particulate Organic Matter Dry"),
        (62012, "Secondary Particulate Organic Matter Dry"),
        (62013, "Black Carbon Hydrophilic Dry"),
        (62014, "Black Carbon Hydrophobic Dry"),
        (62015, "Particulate Organic Matter Hydrophilic Dry"),
        (62016, "Particulate Organic Matter Hydrophobic Dry"),
        (62017, "Nitrate Hydrophilic Dry"),
        (62018, "Nitrate Hydrophobic Dry"),
        (62020, "Smoke High Absorption"),
        (62021, "Smoke Low Absorption"),
        (62022, "Aerosol High Absorption"),
        (62023, "Aerosol Low Absorption"),
        (62025, "Volcanic Ash"),
        (62026, "Particulate Matter"),
        (62028, "Total Aerosol Hydrophilic"),
        (62029, "Total Aerosol Hydrophobic"),
        (62030, "Primary Particulate Inorganic Matter Dry"),
        (62031, "Secondary Particulate Inorganic Matter Dry"),
        (62032, "Biogenic Secondary Organic Aerosol"),
        (62033, "Anthropogenic Secondary Organic Aerosol"),
        (62034, "Rain Water"),
        (62035, "Cloud Water"),
        (62036, "Brown Carbon Dry"),
        (62037, "Sea Salt Wet at 80% Relative Humidity"),
    ];

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
