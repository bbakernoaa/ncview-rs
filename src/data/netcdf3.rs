use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Mutex,
};

use ndarray::Array2;
use netcdf_reader::{NcAttrValue, NcFile, NcSliceInfo, NcSliceInfoElem, NcType};

use super::slice::{PackedAttributes, Slice2D, SliceRequest, classify_packed};
use super::{
    AxisRole, DataSource, DatasetFormat, DatasetMetadata, Dimension, PointCoordinates, Variable,
};
use crate::error::{NcvError, Result};

pub struct NetCdf3Source {
    path: PathBuf,
    metadata: DatasetMetadata,
    reader: Mutex<NcFile>,
}

pub fn is_netcdf3(path: &Path) -> bool {
    File::open(path)
        .and_then(|mut file| {
            let mut magic = [0_u8; 4];
            std::io::Read::read_exact(&mut file, &mut magic).map(|_| magic)
        })
        .is_ok_and(|magic| &magic[..3] == b"CDF")
}

impl NetCdf3Source {
    pub fn open(path: &Path) -> Result<Self> {
        let reader = NcFile::open(path).map_err(|error| NcvError::InvalidDataset {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
        let metadata = metadata(path, &reader)?;
        Ok(Self {
            path: path.to_path_buf(),
            metadata,
            reader: Mutex::new(reader),
        })
    }

    pub(crate) fn read_variable_values(&self, variable: &str) -> Result<Vec<f64>> {
        let reader = self.reader.lock().map_err(|_| NcvError::Adapter {
            path: self.path.clone(),
            reason: "NetCDF-3 reader lock was poisoned".into(),
        })?;
        let values = reader
            .read_variable_as_f64(variable)
            .map_err(|error| NcvError::Adapter {
                path: self.path.clone(),
                reason: error.to_string(),
            })?;
        Ok(values.into_iter().collect())
    }

    pub(crate) fn read_variable_strings(&self, variable: &str) -> Result<Vec<String>> {
        let reader = self.reader.lock().map_err(|_| NcvError::Adapter {
            path: self.path.clone(),
            reason: "NetCDF-3 reader lock was poisoned".into(),
        })?;
        reader
            .read_variable_as_strings(variable)
            .map_err(|error| NcvError::Adapter {
                path: self.path.clone(),
                reason: error.to_string(),
            })
    }

    pub(crate) fn read_mesh_values(
        &self,
        variable: &str,
        mesh_dimension: &str,
        indices: &super::mpas::Selection,
    ) -> Result<Vec<f64>> {
        let reader = self.reader.lock().map_err(|_| NcvError::Adapter {
            path: self.path.clone(),
            reason: "NetCDF-3 reader lock was poisoned".into(),
        })?;
        let variables = reader.variables().map_err(|error| NcvError::Adapter {
            path: self.path.clone(),
            reason: error.to_string(),
        })?;
        let selected = variables
            .iter()
            .find(|candidate| candidate.name == variable)
            .ok_or_else(|| NcvError::UnsupportedVariable {
                variable: variable.to_owned(),
                reason: "variable not found".into(),
            })?;
        let selections = selected
            .dimensions
            .iter()
            .map(|dimension| {
                if dimension.name == mesh_dimension {
                    Ok(NcSliceInfoElem::Slice {
                        start: 0,
                        end: dimension.size,
                        step: 1,
                    })
                } else {
                    let length = usize::try_from(dimension.size).unwrap_or(usize::MAX);
                    super::mpas::dimension_index(variable, &dimension.name, length, indices)
                        .map(|index| NcSliceInfoElem::Index(index as u64))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let selection = NcSliceInfo { selections };
        read_selected_f64(&reader, variable, &selected.dtype, &selection).map_err(|error| {
            NcvError::Adapter {
                path: self.path.clone(),
                reason: error.to_string(),
            }
        })
    }
}

fn read_selected_f64(
    reader: &NcFile,
    variable: &str,
    dtype: &NcType,
    selection: &NcSliceInfo,
) -> Result<Vec<f64>> {
    macro_rules! read {
        ($type:ty) => {
            reader
                .read_variable_slice::<$type>(variable, selection)
                .map(|values| values.into_iter().map(|value| value as f64).collect())
                .map_err(|error| NcvError::Adapter {
                    path: PathBuf::new(),
                    reason: error.to_string(),
                })
        };
    }
    match dtype {
        NcType::Byte => read!(i8),
        NcType::UByte => read!(u8),
        NcType::Short => read!(i16),
        NcType::UShort => read!(u16),
        NcType::Int => read!(i32),
        NcType::UInt => read!(u32),
        NcType::Float => read!(f32),
        NcType::Double => read!(f64),
        NcType::Int64 => read!(i64),
        NcType::UInt64 => read!(u64),
        _ => Err(NcvError::UnsupportedVariable {
            variable: variable.to_owned(),
            reason: format!("non-numeric NetCDF-3 type {dtype:?}"),
        }),
    }
}

impl DataSource for NetCdf3Source {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        let variable = self
            .metadata
            .variables
            .iter()
            .find(|variable| variable.name == request.variable)
            .ok_or_else(|| NcvError::UnsupportedVariable {
                variable: request.variable.clone(),
                reason: "variable not found".into(),
            })?;
        if variable.dimensions.len() < 2 {
            return Err(NcvError::UnsupportedVariable {
                variable: request.variable.clone(),
                reason: "a plottable variable needs at least two dimensions".into(),
            });
        }
        let reader = self.reader.lock().map_err(|_| NcvError::Adapter {
            path: self.path.clone(),
            reason: "NetCDF-3 reader lock was poisoned".into(),
        })?;
        let variables = reader.variables().map_err(|error| NcvError::Adapter {
            path: self.path.clone(),
            reason: error.to_string(),
        })?;
        let selected = variables
            .iter()
            .find(|candidate| candidate.name == request.variable)
            .ok_or_else(|| NcvError::UnsupportedVariable {
                variable: request.variable.clone(),
                reason: "variable not found".into(),
            })?;
        let rank = selected.dimensions.len();
        let rows = selected.dimensions[rank - 2].size;
        let cols = selected.dimensions[rank - 1].size;
        let bounds = request.bounds;
        if bounds.row_end as u64 > rows || bounds.col_end as u64 > cols {
            return Err(NcvError::InvalidSlice(
                "slice bounds exceed variable shape".into(),
            ));
        }
        let mut selections = Vec::with_capacity(rank);
        for (index, dimension) in selected.dimensions.iter().enumerate() {
            let selection = if index == rank - 2 {
                NcSliceInfoElem::Slice {
                    start: bounds.row_start as u64,
                    end: bounds.row_end as u64,
                    step: 1,
                }
            } else if index == rank - 1 {
                NcSliceInfoElem::Slice {
                    start: bounds.col_start as u64,
                    end: bounds.col_end as u64,
                    step: 1,
                }
            } else {
                let index_value = match role_for_name(&dimension.name) {
                    AxisRole::Time => request.time,
                    AxisRole::Depth => request.depth,
                    _ if dimension.size == 1 => 0,
                    _ => {
                        return Err(NcvError::UnsupportedVariable {
                            variable: request.variable.clone(),
                            reason: format!("cannot select dimension {}", dimension.name),
                        });
                    }
                };
                if index_value as u64 >= dimension.size {
                    return Err(NcvError::InvalidSlice(format!(
                        "index exceeds dimension {}",
                        dimension.name
                    )));
                }
                NcSliceInfoElem::Index(index_value as u64)
            };
            selections.push(selection);
        }
        let raw = read_selected_f64(
            &reader,
            &request.variable,
            &selected.dtype,
            &NcSliceInfo { selections },
        )?;
        let attr = |name: &str| {
            selected
                .attributes
                .iter()
                .find(|item| item.name == name)
                .and_then(|item| item.value.as_f64())
        };
        let valid_range = selected
            .attributes
            .iter()
            .find(|item| item.name == "valid_range")
            .and_then(|item| item.value.as_f64_vec());
        let packed = PackedAttributes {
            fill: attr("_FillValue"),
            missing: attr("missing_value"),
            valid_min: attr("valid_min")
                .or_else(|| valid_range.as_ref().and_then(|v| v.first().copied())),
            valid_max: attr("valid_max")
                .or_else(|| valid_range.as_ref().and_then(|v| v.get(1).copied())),
            scale_factor: attr("scale_factor"),
            add_offset: attr("add_offset"),
        };
        let (values, validity) = classify_packed(&raw, packed);
        let values = Array2::from_shape_vec(bounds.shape(), values)
            .map_err(|error| NcvError::InvalidSlice(error.to_string()))?;
        let validity = Array2::from_shape_vec(bounds.shape(), validity)
            .map_err(|error| NcvError::InvalidSlice(error.to_string()))?;
        Slice2D::new(values, validity, bounds)
    }

    fn time_label(&self, _index: usize) -> Option<String> {
        None
    }

    fn vertical_label(&self, _variable: &str, _index: usize) -> Option<String> {
        None
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        let metadata = self
            .metadata
            .variables
            .iter()
            .find(|item| item.name == variable)?;
        if !metadata.dimensions.iter().any(|name| name == dimension) {
            return None;
        }
        let coordinate = self.metadata.variables.iter().find(|item| {
            item.name == dimension && item.numeric && item.dimensions == [dimension]
        })?;
        self.read_variable_values(&coordinate.name).ok()
    }

    fn point_coordinates(&self, _variable: &str, _row: usize, _col: usize) -> PointCoordinates {
        PointCoordinates::default()
    }
}

fn metadata(path: &Path, file: &NcFile) -> Result<DatasetMetadata> {
    let dimensions = file
        .dimensions()
        .map_err(|error| NcvError::InvalidDataset {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?
        .iter()
        .map(|dimension| Dimension {
            name: dimension.name.clone(),
            length: usize::try_from(dimension.size).unwrap_or(usize::MAX),
            role: role_for_name(&dimension.name),
        })
        .collect::<Vec<_>>();
    let variables = file
        .variables()
        .map_err(|error| NcvError::InvalidDataset {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?
        .iter()
        .map(|variable| Variable {
            name: variable.name.clone(),
            dimensions: variable
                .dimensions
                .iter()
                .map(|dimension| dimension.name.clone())
                .collect(),
            numeric: is_numeric_type(&variable.dtype),
            units: attribute_text(variable.attributes.as_slice(), "units"),
            long_name: attribute_text(variable.attributes.as_slice(), "long_name"),
            standard_name: attribute_text(variable.attributes.as_slice(), "standard_name"),
        })
        .collect();
    Ok(DatasetMetadata {
        path: path.display().to_string(),
        format: DatasetFormat::NetCdf3,
        dimensions,
        variables,
    })
}

fn attribute_text(attributes: &[netcdf_reader::NcAttribute], name: &str) -> Option<String> {
    attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .and_then(|attribute| match &attribute.value {
            NcAttrValue::Chars(value) => Some(value.clone()),
            NcAttrValue::Strings(values) => values.first().cloned(),
            _ => None,
        })
}

fn is_numeric_type(dtype: &NcType) -> bool {
    matches!(
        dtype,
        NcType::Byte
            | NcType::Short
            | NcType::Int
            | NcType::Float
            | NcType::Double
            | NcType::UByte
            | NcType::UShort
            | NcType::UInt
            | NcType::Int64
            | NcType::UInt64
    )
}

fn role_for_name(name: &str) -> AxisRole {
    let lower = name.to_ascii_lowercase();
    if lower.contains("time") || lower == "date" || lower == "dates" {
        AxisRole::Time
    } else if lower.contains("depth") || lower.contains("lev") || lower.contains("level") {
        AxisRole::Depth
    } else if lower.contains("lat") {
        AxisRole::Latitude
    } else if lower.contains("lon") {
        AxisRole::Longitude
    } else {
        AxisRole::Other
    }
}
