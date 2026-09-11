//! Shared GRIB2 metadata and diagnostic types.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grib2SourceLocation {
    pub path: String,
    pub message: usize,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableResolution {
    Defined {
        table: String,
        code: u64,
        label: String,
        unit: Option<String>,
    },
    Raw {
        table: String,
        code: u64,
        reason: RawCodeReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawCodeReason {
    Unknown,
    Reserved,
    LocalUse,
    Missing,
    Future,
}

impl fmt::Display for RawCodeReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Unknown => "unknown",
            Self::Reserved => "reserved",
            Self::LocalUse => "local-use",
            Self::Missing => "missing",
            Self::Future => "future",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grib2MessageHeader {
    pub location: Grib2SourceLocation,
    pub discipline: u8,
    pub centre: u16,
    pub subcentre: u16,
    pub master_table_version: u8,
    pub local_table_version: u8,
    pub grid_template: u16,
    pub product_template: u16,
    pub data_representation_template: u16,
    pub grid_points: usize,
    /// Raw Section 4 payload bytes (including the coordinate-count and
    /// template-number prefix). Keeping this payload is intentional: WMO
    /// and centre-local product templates can carry aerosol species,
    /// particle-size intervals, wavelengths, and other qualifiers that are
    /// not represented by a short name.
    pub product_definition_payload: Vec<u8>,
    pub parameter_category: Option<u8>,
    pub parameter_number: Option<u8>,
    pub description: String,
    pub resolution: Vec<TableResolution>,
}

impl Grib2MessageHeader {
    pub fn raw_code_label(&self) -> String {
        format!(
            "d{}-c{}-p{}-g{}-t{}-r{}",
            self.discipline,
            self.parameter_category
                .map_or_else(|| "x".into(), |value| value.to_string()),
            self.parameter_number
                .map_or_else(|| "x".into(), |value| value.to_string()),
            self.grid_template,
            self.product_template,
            self.data_representation_template
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grib2Diagnostic {
    pub code: &'static str,
    pub source: Option<Grib2SourceLocation>,
    pub message: String,
}

impl Grib2Diagnostic {
    pub fn new(
        code: &'static str,
        source: Option<Grib2SourceLocation>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            source,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_code_label_is_deterministic() {
        let header = Grib2MessageHeader {
            location: Grib2SourceLocation {
                path: "field.grib2".into(),
                message: 0,
                offset: Some(0),
            },
            discipline: 0,
            centre: 7,
            subcentre: 0,
            master_table_version: 1,
            local_table_version: 0,
            grid_template: 0,
            product_template: 0,
            data_representation_template: 0,
            grid_points: 4,
            product_definition_payload: vec![0, 0, 0, 0, 13, 193],
            parameter_category: Some(13),
            parameter_number: Some(193),
            description: "aerosol optical thickness".into(),
            resolution: vec![TableResolution::Raw {
                table: "4.2-0-13".into(),
                code: 193,
                reason: RawCodeReason::Unknown,
            }],
        };
        assert_eq!(header.raw_code_label(), "d0-c13-p193-g0-t0-r0");
    }
}
