//! Stable logical identities for GRIB2 fields.

use super::grib2_types::Grib2MessageHeader;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AerosolQualifierSet {
    pub species: Option<String>,
    pub first_size: Option<String>,
    pub second_size: Option<String>,
    pub first_wavelength: Option<String>,
    pub second_wavelength: Option<String>,
    pub optical_context: Option<String>,
}

impl AerosolQualifierSet {
    fn components(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("species", self.species.as_deref()),
            ("size1", self.first_size.as_deref()),
            ("size2", self.second_size.as_deref()),
            ("wavelength1", self.first_wavelength.as_deref()),
            ("wavelength2", self.second_wavelength.as_deref()),
            ("optical", self.optical_context.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIdentity {
    pub machine_key: String,
    pub display_label: String,
    pub human_name: String,
    pub short_name: String,
    pub aerosol: AerosolQualifierSet,
}

impl FieldIdentity {
    pub fn from_header(header: &Grib2MessageHeader) -> Self {
        let human_base = readable_name(&header.description);
        let short_name = human_base
            .split('_')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("grib2")
            .to_owned();
        let aerosol = aerosol_qualifiers(header);
        let mut components = vec![
            format!("discipline={}", header.discipline),
            format!("centre={}", header.centre),
            format!("subcentre={}", header.subcentre),
            format!("master={}", header.master_table_version),
            format!("local={}", header.local_table_version),
            format!("grid={}", header.grid_template),
            format!("product={}", header.product_template),
            format!("repr={}", header.data_representation_template),
            format!("category={}", display_code(header.parameter_category)),
            format!("parameter={}", display_code(header.parameter_number)),
            format!(
                "product-bytes={}",
                hex_bytes(&header.product_definition_payload)
            ),
        ];
        components.extend(
            aerosol
                .components()
                .map(|(name, value)| format!("{name}={}", escape(value))),
        );
        let human_name = aerosol_human_name(&human_base, &aerosol);
        Self {
            machine_key: format!("grib2/v1/{}", components.join(";")),
            display_label: human_name.clone(),
            human_name,
            short_name,
            aerosol,
        }
    }

    pub fn with_aerosol_qualifiers(mut self, aerosol: AerosolQualifierSet) -> Self {
        self.aerosol = aerosol;
        let qualifier_text = self
            .aerosol
            .components()
            .map(|(name, value)| format!("{name}={}", escape(value)))
            .collect::<Vec<_>>();
        if !qualifier_text.is_empty() {
            self.machine_key.push(';');
            self.machine_key.push_str(&qualifier_text.join(";"));
            self.human_name = aerosol_human_name(&self.short_name, &self.aerosol);
            self.display_label = self.human_name.clone();
        }
        self
    }
}

/// Return the aerosol type code from the common aerosol product-template
/// prefix. The Section 4 payload starts with the coordinate count and
/// template number, followed by category/parameter; aerosol type is the next
/// two octets for the aerosol templates.
pub fn aerosol_type_code(header: &Grib2MessageHeader) -> Option<u16> {
    aerosol_type_code_for(header.product_template, &header.product_definition_payload)
}

pub fn aerosol_type_code_for(product_template: u16, payload: &[u8]) -> Option<u16> {
    is_aerosol_template(product_template)
        .then(|| payload.get(6..8))
        .flatten()
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn aerosol_qualifiers(header: &Grib2MessageHeader) -> AerosolQualifierSet {
    let Some(code) = aerosol_type_code(header) else {
        return AerosolQualifierSet::default();
    };
    let payload = &header.product_definition_payload;
    let mut qualifiers = AerosolQualifierSet {
        species: Some(
            header
                .resolution
                .iter()
                .find_map(|resolution| match resolution {
                    super::grib2_types::TableResolution::Defined {
                        table,
                        code: resolution_code,
                        label,
                        ..
                    } if table == "4.233" && *resolution_code == u64::from(code) => {
                        Some(label.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| format!("code-{code}")),
        ),
        optical_context: Some(format!("template-4.{}", header.product_template)),
        ..AerosolQualifierSet::default()
    };
    if let Some(size_fields) = scaled_interval(payload, 8, QuantityKind::Size) {
        qualifiers.first_size = Some(size_fields.0);
        qualifiers.second_size = Some(size_fields.1);
    }
    if has_wavelengths(header.product_template)
        && let Some(wavelength_fields) = scaled_interval(payload, 19, QuantityKind::Wavelength)
    {
        qualifiers.first_wavelength = Some(wavelength_fields.0);
        qualifiers.second_wavelength = Some(wavelength_fields.1);
    }
    qualifiers
}

#[derive(Clone, Copy)]
enum QuantityKind {
    Size,
    Wavelength,
}

fn scaled_interval(
    payload: &[u8],
    interval_offset: usize,
    quantity: QuantityKind,
) -> Option<(String, String)> {
    let interval = *payload.get(interval_offset)?;
    let first_scale = i8::from_be_bytes([*payload.get(interval_offset + 1)?]);
    let first_value = i32::from_be_bytes(
        payload
            .get(interval_offset + 2..interval_offset + 6)?
            .try_into()
            .ok()?,
    );
    let second_scale = i8::from_be_bytes([*payload.get(interval_offset + 6)?]);
    let second_value = i32::from_be_bytes(
        payload
            .get(interval_offset + 7..interval_offset + 11)?
            .try_into()
            .ok()?,
    );
    Some((
        format!(
            "interval={interval};{}",
            scaled_value(first_scale, first_value, quantity)
        ),
        format!(
            "interval={interval};{}",
            scaled_value(second_scale, second_value, quantity)
        ),
    ))
}

fn scaled_value(scale: i8, value: i32, quantity: QuantityKind) -> String {
    if value == i32::MIN {
        "missing".into()
    } else {
        let value = f64::from(value) * 10_f64.powi(-i32::from(scale));
        match quantity {
            QuantityKind::Size => {
                if value.abs() < 1e-3 {
                    format_quantity(value * 1e6, "um")
                } else {
                    format_quantity(value, "m")
                }
            }
            QuantityKind::Wavelength => {
                if value.abs() < 1e-3 {
                    format_quantity(value * 1e9, "nm")
                } else {
                    format_quantity(value, "m")
                }
            }
        }
    }
}

fn format_quantity(value: f64, unit: &str) -> String {
    let mut number = format!("{value:.6}");
    while number.ends_with('0') {
        number.pop();
    }
    if number.ends_with('.') {
        number.pop();
    }
    format!("{number}{unit}")
}

fn readable_name(description: &str) -> String {
    let source = description
        .lines()
        .find_map(|line| line.strip_prefix("  Parameter:"))
        .unwrap_or(description)
        .trim();
    let mut name = String::new();
    for character in source.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_matches('_').to_owned();
    if name.is_empty() {
        "grib2_field".into()
    } else {
        name
    }
}

fn aerosol_human_name(base: &str, aerosol: &AerosolQualifierSet) -> String {
    let Some(species) = aerosol.species.as_deref() else {
        return base.to_owned();
    };
    let mut parts = vec![slug(species)];
    if let (Some(first), Some(second)) = (
        aerosol.first_size.as_deref(),
        aerosol.second_size.as_deref(),
    ) {
        parts.push(compact_interval(first, second));
    }
    parts.push(base.to_owned());
    if let (Some(first), Some(second)) = (
        aerosol.first_wavelength.as_deref(),
        aerosol.second_wavelength.as_deref(),
    ) {
        parts.push(compact_interval(first, second));
    }
    parts.join("_")
}

fn compact_quantity(value: &str) -> String {
    let value = value.split(';').nth(1).unwrap_or(value);
    slug(value)
}

fn compact_interval(first: &str, second: &str) -> String {
    let interval = first
        .strip_prefix("interval=")
        .and_then(|value| value.split(';').next())
        .and_then(|value| value.parse::<u8>().ok());
    let first = compact_quantity(first);
    let second = compact_quantity(second);
    match interval {
        Some(0) => format!("lt{first}"),
        Some(1) => format!("gt{second}"),
        Some(2 | 7 | 10) => format!("{first}-{second}"),
        Some(3) => format!("gt{first}"),
        Some(4) => format!("lt{second}"),
        Some(5) => format!("le{first}"),
        Some(6) => format!("ge{second}"),
        Some(8) => format!("ge{first}"),
        Some(9) => format!("le{second}"),
        Some(11) => format!("eq{first}"),
        Some(other) => format!("interval{other}_{first}-{second}"),
        None => format!("{first}-{second}"),
    }
}

pub(crate) fn slug(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
        } else if character == '.' {
            result.push('p');
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    result.trim_matches('_').to_owned()
}

fn is_aerosol_template(template: u16) -> bool {
    matches!(
        template,
        44..=50
            | 80..=85
            | 156..=159
            | 168..=176
            | 179..=187
            | 190..=198
    )
}

fn has_wavelengths(template: u16) -> bool {
    matches!(
        template,
        48 | 49
            | 80
            | 81
            | 156
            | 157
            | 158
            | 159
            | 169
            | 172
            | 175
            | 176
            | 180
            | 183
            | 186
            | 187
            | 191
            | 194
            | 197
            | 198
    )
}

fn display_code(value: Option<u8>) -> String {
    value.map_or_else(|| "missing".into(), |value| value.to_string())
}

fn escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(';', "%3B")
        .replace('=', "%3D")
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::grib2_types::{Grib2SourceLocation, RawCodeReason, TableResolution};

    fn header() -> Grib2MessageHeader {
        Grib2MessageHeader {
            location: Grib2SourceLocation {
                path: "a.grib2".into(),
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
            product_definition_payload: vec![0, 0, 0, 0, 193, 0],
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
    fn identity_does_not_use_short_name_as_key() {
        let identity = FieldIdentity::from_header(&header());
        assert!(identity.machine_key.contains("parameter=193"));
        assert_ne!(identity.machine_key, "AOTK");
    }

    #[test]
    fn aerosol_qualifiers_are_escaped_and_stable() {
        let identity =
            FieldIdentity::from_header(&header()).with_aerosol_qualifiers(AerosolQualifierSet {
                species: Some("dust;coarse".into()),
                first_size: Some("0.1m".into()),
                ..AerosolQualifierSet::default()
            });
        assert!(identity.machine_key.contains("dust%3Bcoarse"));
        assert_eq!(identity.human_name, "dust_coarse_aotk");
        assert_eq!(identity.display_label, identity.human_name);
    }
}
