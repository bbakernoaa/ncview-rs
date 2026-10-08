//! Derived variables defined by equation-editor formulas.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::{
    DataSource, DatasetMetadata, PointCoordinates, Variable,
    slice::{Bounds, Slice2D, SliceRequest},
    virtual_dataset::{leading_axes, leading_lengths},
};
use crate::analysis::formula::{self, AggregateAxis, Expr, FormulaInputs, VarRef};
use crate::error::{NcvError, Result};

/// A parsed `name = expression` (or bare expression) entered by the user.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaDefinition {
    pub name: String,
    pub expression: String,
    pub expr: Expr,
}

impl FormulaDefinition {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let (name, expression) = match text.split_once('=') {
            Some((name, expression)) => {
                let name = name.trim();
                if !formula::is_identifier(name) {
                    return Err(NcvError::Formula(format!(
                        "{name:?} is not a valid formula name (use letters, digits, _ or .)"
                    )));
                }
                (name.to_owned(), expression.trim().to_owned())
            }
            None => (text.to_owned(), text.to_owned()),
        };
        let expr = formula::parse(&expression)?;
        Ok(Self {
            name,
            expression,
            expr,
        })
    }

    /// Zero-based dataset that owns a formula whose references are all
    /// dataset-qualified; `None` when it applies to every dataset.
    pub fn anchor(&self) -> Option<usize> {
        let references = formula::references(&self.expr);
        if references
            .iter()
            .any(|reference| reference.dataset.is_none())
        {
            return None;
        }
        references
            .first()
            .and_then(|reference| reference.dataset)
            .map(|dataset| dataset - 1)
    }
}

struct CompiledFormula {
    name: String,
    expr: Expr,
    primary: VarRef,
    time_aggregated: bool,
}

/// Wraps a dataset and exposes formula results as additional variables.
pub struct FormulaSource {
    base: Arc<dyn DataSource>,
    datasets: Vec<Arc<dyn DataSource>>,
    formulas: Vec<CompiledFormula>,
    metadata: DatasetMetadata,
}

impl FormulaSource {
    /// `datasets` are the opened files addressed by `NAME[n]` (1-based);
    /// unqualified names resolve against `base`.
    pub fn new(
        base: Arc<dyn DataSource>,
        datasets: Vec<Arc<dyn DataSource>>,
        definitions: &[FormulaDefinition],
    ) -> Result<Self> {
        let mut source = Self {
            metadata: base.metadata().clone(),
            base,
            datasets,
            formulas: Vec::new(),
        };
        for definition in definitions {
            source.add(definition)?;
        }
        Ok(source)
    }

    fn resolve(&self, reference: &VarRef) -> Result<(&dyn DataSource, &Variable)> {
        let source = match reference.dataset {
            None => self.base.as_ref(),
            Some(index) => self
                .datasets
                .get(index - 1)
                .ok_or_else(|| {
                    NcvError::Formula(format!(
                        "dataset [{index}] is not open ({} dataset(s) loaded)",
                        self.datasets.len()
                    ))
                })?
                .as_ref(),
        };
        let variable = source
            .metadata()
            .variables
            .iter()
            .find(|variable| variable.name == reference.name)
            .filter(|variable| variable.numeric && variable.dimensions.len() >= 2)
            .ok_or_else(|| {
                NcvError::Formula(format!(
                    "{} is not a plottable variable in {}",
                    reference.name,
                    source.metadata().path
                ))
            })?;
        Ok((source, variable))
    }

    fn add(&mut self, definition: &FormulaDefinition) -> Result<()> {
        if self
            .metadata
            .variables
            .iter()
            .any(|variable| variable.name == definition.name)
        {
            return Err(NcvError::Formula(format!(
                "{} is already a variable name",
                definition.name
            )));
        }
        let references = formula::references(&definition.expr);
        let primary = references.first().cloned().ok_or_else(|| {
            NcvError::Formula("formula must reference at least one variable".into())
        })?;
        for reference in &references {
            self.resolve(reference)?;
        }
        let (primary_source, primary_variable) = self.resolve(&primary)?;
        let primary_metadata = primary_source.metadata();
        let (time_axis, depth_axis) = leading_axes(primary_metadata, primary_variable);
        let time_aggregated = !formula::depends_on(&definition.expr, AggregateAxis::Time);
        let layer_aggregated = !formula::depends_on(&definition.expr, AggregateAxis::Layer);
        let dimensions = primary_variable
            .dimensions
            .iter()
            .enumerate()
            .filter(|(axis, _)| {
                !(time_aggregated && Some(*axis) == time_axis
                    || layer_aggregated && Some(*axis) == depth_axis)
            })
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
        if dimensions.len() < 2 {
            return Err(NcvError::Formula(
                "formula result needs two spatial dimensions".into(),
            ));
        }
        let new_dimensions = dimensions
            .iter()
            .filter(|name| {
                !self
                    .metadata
                    .dimensions
                    .iter()
                    .any(|dimension| dimension.name == **name)
            })
            .filter_map(|name| {
                primary_metadata
                    .dimensions
                    .iter()
                    .find(|dimension| dimension.name == *name)
                    .cloned()
            })
            .collect::<Vec<_>>();
        self.metadata.dimensions.extend(new_dimensions);
        self.metadata.variables.push(Variable {
            name: definition.name.clone(),
            dimensions,
            numeric: true,
            units: None,
            long_name: Some(definition.expression.clone()),
            standard_name: None,
        });
        self.formulas.push(CompiledFormula {
            name: definition.name.clone(),
            expr: definition.expr.clone(),
            primary,
            time_aggregated,
        });
        Ok(())
    }

    fn formula(&self, name: &str) -> Option<&CompiledFormula> {
        self.formulas.iter().find(|formula| formula.name == name)
    }

    /// Source and variable name that supply labels and coordinates for `variable`.
    fn label_target<'a>(&'a self, variable: &'a str) -> (&'a dyn DataSource, &'a str) {
        self.formula(variable)
            .and_then(|formula| {
                self.resolve(&formula.primary)
                    .ok()
                    .map(|(source, _)| (source, formula.primary.name.as_str()))
            })
            .unwrap_or((self.base.as_ref(), variable))
    }
}

struct SliceInputs<'a> {
    owner: &'a FormulaSource,
    bounds: Bounds,
    row_axis: Option<&'a str>,
    col_axis: Option<&'a str>,
    fixed_axes: &'a [(String, usize)],
    cancelled: Arc<AtomicBool>,
}

impl FormulaInputs for SliceInputs<'_> {
    fn read(&self, reference: &VarRef, time: usize, depth: usize) -> Result<Slice2D> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(NcvError::WorkerStopped);
        }
        let (source, variable) = self.owner.resolve(reference)?;
        let fixed_axes = match (self.row_axis, self.col_axis) {
            (Some(row_axis), Some(col_axis)) => {
                let metadata = source.metadata();
                let (time_axis, depth_axis) = leading_axes(metadata, variable);
                variable
                    .dimensions
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| {
                        !name.eq_ignore_ascii_case(row_axis) && !name.eq_ignore_ascii_case(col_axis)
                    })
                    .map(|(axis, name)| {
                        let index = if Some(axis) == time_axis {
                            time
                        } else if Some(axis) == depth_axis {
                            depth
                        } else {
                            self.fixed_axes
                                .iter()
                                .find(|(fixed, _)| fixed.eq_ignore_ascii_case(name))
                                .map_or(0, |(_, index)| *index)
                        };
                        let length = metadata
                            .dimensions
                            .iter()
                            .find(|dimension| dimension.name == *name)
                            .map_or(1, |dimension| dimension.length);
                        (name.clone(), index.min(length.saturating_sub(1)))
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        source.read_slice_on_axes_cancellable(
            &SliceRequest {
                variable: reference.name.clone(),
                time,
                depth,
                bounds: self.bounds,
            },
            self.row_axis,
            self.col_axis,
            &fixed_axes,
            Arc::clone(&self.cancelled),
        )
    }

    fn lengths(&self, reference: &VarRef) -> Result<(usize, usize)> {
        let (source, variable) = self.owner.resolve(reference)?;
        Ok(leading_lengths(source.metadata(), variable))
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl DataSource for FormulaSource {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.read_slice_on_axes_cancellable(
            request,
            None,
            None,
            &[],
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn is_remote(&self) -> bool {
        self.base.is_remote()
    }

    fn source_identity(&self) -> Option<&str> {
        self.base.source_identity()
    }

    fn remote_capabilities(&self) -> Option<super::remote::AccessCapabilities> {
        self.base.remote_capabilities()
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        self.read_slice_on_axes_cancellable(
            request,
            row_axis,
            col_axis,
            fixed_axes,
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn read_slice_on_axes_cancellable(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
        cancelled: Arc<AtomicBool>,
    ) -> Result<Slice2D> {
        let Some(formula) = self.formula(&request.variable) else {
            return self.base.read_slice_on_axes_cancellable(
                request, row_axis, col_axis, fixed_axes, cancelled,
            );
        };
        let inputs = SliceInputs {
            owner: self,
            bounds: request.bounds,
            row_axis,
            col_axis,
            fixed_axes,
            cancelled,
        };
        formula::evaluate(&formula.expr, &inputs, request.time, request.depth)
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.base.time_label(index)
    }

    fn time_label_for_variable(&self, variable: &str, index: usize) -> Option<String> {
        if let Some(formula) = self.formula(variable)
            && formula.time_aggregated
        {
            let (source, variable) = self.resolve(&formula.primary).ok()?;
            let steps = leading_lengths(source.metadata(), variable).0;
            let first = source.time_label_for_variable(&variable.name, 0)?;
            let last = source.time_label_for_variable(&variable.name, steps.saturating_sub(1))?;
            return Some(format!("{first} to {last}"));
        }
        let (source, name) = self.label_target(variable);
        source.time_label_for_variable(name, index)
    }

    fn vertical_label(&self, variable: &str, index: usize) -> Option<String> {
        let (source, name) = self.label_target(variable);
        source.vertical_label(name, index)
    }

    fn vertical_labels(&self, variable: &str) -> Vec<String> {
        let (source, name) = self.label_target(variable);
        source.vertical_labels(name)
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        let (source, name) = self.label_target(variable);
        source.dimension_values(name, dimension)
    }

    fn point_coordinates(&self, variable: &str, row: usize, col: usize) -> PointCoordinates {
        let (source, name) = self.label_target(variable);
        source.point_coordinates(name, row, col)
    }
}

/// Attach formulas to opened datasets.
///
/// Formulas with any unqualified reference are applied per dataset; formulas
/// whose references are all `NAME[n]` belong only to the first referenced
/// dataset so a multi-file collection does not show the same field twice.
/// Returns the wrapped sources and one message per formula that no dataset
/// could evaluate.
pub fn apply_formulas(
    sources: &[Arc<dyn DataSource>],
    definitions: &[FormulaDefinition],
) -> (Vec<Arc<dyn DataSource>>, Vec<String>) {
    let mut first_error: Vec<Option<String>> = vec![None; definitions.len()];
    let mut accepted_anywhere = vec![false; definitions.len()];
    let wrapped = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let mut accepted: Vec<FormulaDefinition> = Vec::new();
            for (position, definition) in definitions.iter().enumerate() {
                if definition.anchor().is_some_and(|anchor| anchor != index) {
                    continue;
                }
                let mut trial = accepted.clone();
                trial.push(definition.clone());
                match FormulaSource::new(Arc::clone(source), sources.to_vec(), &trial) {
                    Ok(_) => {
                        accepted = trial;
                        accepted_anywhere[position] = true;
                    }
                    Err(error) => {
                        first_error[position].get_or_insert_with(|| error.to_string());
                    }
                }
            }
            if accepted.is_empty() {
                return Arc::clone(source);
            }
            match FormulaSource::new(Arc::clone(source), sources.to_vec(), &accepted) {
                Ok(formula_source) => Arc::new(formula_source) as Arc<dyn DataSource>,
                Err(_) => Arc::clone(source),
            }
        })
        .collect();
    let errors = definitions
        .iter()
        .enumerate()
        .filter(|(position, _)| !accepted_anywhere[*position])
        .map(|(position, definition)| {
            format!(
                "{}: {}",
                definition.name,
                first_error[position]
                    .clone()
                    .unwrap_or_else(|| "dataset is not open".into())
            )
        })
        .collect();
    (wrapped, errors)
}
