//! Equation-editor formulas: parsing and grid-cell evaluation.
//!
//! The grammar follows the VERDI formula editor: arithmetic (`+ - * / ^ **`),
//! elementary functions, and per-cell aggregation over time or vertical
//! layers. `NAME[n]` references variable `NAME` in the n-th opened dataset
//! (1-based); an unqualified `NAME` refers to the dataset being viewed.

use ndarray::{Array2, Zip};
use std::sync::Arc;

use crate::data::slice::{Bounds, CoordinateGrid, Slice2D, Validity};
use crate::error::{NcvError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarRef {
    pub name: String,
    pub dataset: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    Sin,
    Cos,
    Tan,
    Log,
    Log10,
    Exp,
    Sqrt,
    Abs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateAxis {
    Time,
    Layer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    TimeMean,
    TimeSum,
    TimeMin,
    TimeMax,
    LayerMean,
    LayerSum,
    LayerMin,
    LayerMax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reduction {
    Mean,
    Sum,
    Min,
    Max,
}

impl Aggregate {
    pub fn axis(self) -> AggregateAxis {
        match self {
            Self::TimeMean | Self::TimeSum | Self::TimeMin | Self::TimeMax => AggregateAxis::Time,
            _ => AggregateAxis::Layer,
        }
    }

    fn reduction(self) -> Reduction {
        match self {
            Self::TimeMean | Self::LayerMean => Reduction::Mean,
            Self::TimeSum | Self::LayerSum => Reduction::Sum,
            Self::TimeMin | Self::LayerMin => Reduction::Min,
            Self::TimeMax | Self::LayerMax => Reduction::Max,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Variable(VarRef),
    Negate(Box<Expr>),
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Function {
        function: Function,
        argument: Box<Expr>,
    },
    Aggregate {
        kind: Aggregate,
        argument: Box<Expr>,
    },
}

/// Names accepted by the parser, listed in the formula editor and docs.
pub const FUNCTION_NAMES: &[&str] = &[
    "sin",
    "cos",
    "tan",
    "log",
    "ln",
    "log10",
    "exp",
    "sqrt",
    "abs",
    "mean",
    "sum",
    "min",
    "max",
    "layer_mean",
    "layer_sum",
    "layer_min",
    "layer_max",
];

enum Callable {
    Function(Function),
    Aggregate(Aggregate),
}

fn callable(name: &str) -> Option<Callable> {
    Some(match name.to_ascii_lowercase().as_str() {
        "sin" => Callable::Function(Function::Sin),
        "cos" => Callable::Function(Function::Cos),
        "tan" => Callable::Function(Function::Tan),
        "log" | "ln" => Callable::Function(Function::Log),
        "log10" => Callable::Function(Function::Log10),
        "exp" => Callable::Function(Function::Exp),
        "sqrt" => Callable::Function(Function::Sqrt),
        "abs" => Callable::Function(Function::Abs),
        "mean" => Callable::Aggregate(Aggregate::TimeMean),
        "sum" => Callable::Aggregate(Aggregate::TimeSum),
        "min" => Callable::Aggregate(Aggregate::TimeMin),
        "max" => Callable::Aggregate(Aggregate::TimeMax),
        "layer_mean" => Callable::Aggregate(Aggregate::LayerMean),
        "layer_sum" => Callable::Aggregate(Aggregate::LayerSum),
        "layer_min" => Callable::Aggregate(Aggregate::LayerMin),
        "layer_max" => Callable::Aggregate(Aggregate::LayerMax),
        _ => return None,
    })
}

fn error(message: impl Into<String>) -> NcvError {
    NcvError::Formula(message.into())
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

/// Characters that may appear in an unquoted variable name after the first.
pub fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '.')
}

/// Whether `text` is a plain identifier usable as a formula name.
pub fn is_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(is_identifier_char)
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let characters = text.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        match character {
            ' ' | '\t' | '\n' | '\r' => index += 1,
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '*' if characters.get(index + 1) == Some(&'*') => {
                tokens.push(Token::Caret);
                index += 2;
            }
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                index += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                index += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                index += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                index += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            '"' | '\'' => {
                let end = characters[index + 1..]
                    .iter()
                    .position(|candidate| *candidate == character)
                    .ok_or_else(|| error(format!("unterminated quoted name at {index}")))?;
                let name = characters[index + 1..index + 1 + end]
                    .iter()
                    .collect::<String>();
                if name.is_empty() {
                    return Err(error("empty quoted name"));
                }
                tokens.push(Token::Ident(name));
                index += end + 2;
            }
            _ if character.is_ascii_digit() || character == '.' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_ascii_digit() || characters[index] == '.')
                {
                    index += 1;
                }
                if index < characters.len() && matches!(characters[index], 'e' | 'E') {
                    let mut lookahead = index + 1;
                    if lookahead < characters.len() && matches!(characters[lookahead], '+' | '-') {
                        lookahead += 1;
                    }
                    if lookahead < characters.len() && characters[lookahead].is_ascii_digit() {
                        index = lookahead;
                        while index < characters.len() && characters[index].is_ascii_digit() {
                            index += 1;
                        }
                    }
                }
                let literal = characters[start..index].iter().collect::<String>();
                let value = literal
                    .parse::<f64>()
                    .map_err(|_| error(format!("invalid number {literal:?}")))?;
                tokens.push(Token::Number(value));
            }
            _ if character.is_ascii_alphabetic() || character == '_' => {
                let start = index;
                while index < characters.len() && is_identifier_char(characters[index]) {
                    index += 1;
                }
                tokens.push(Token::Ident(characters[start..index].iter().collect()));
            }
            _ => {
                return Err(error(format!(
                    "unexpected character {character:?} at {index}"
                )));
            }
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        self.position += 1;
        token
    }

    fn expect(&mut self, expected: &Token, context: &str) -> Result<()> {
        match self.next() {
            Some(token) if token == *expected => Ok(()),
            Some(token) => Err(error(format!("expected {context}, found {token:?}"))),
            None => Err(error(format!("expected {context}, found end of formula"))),
        }
    }

    fn expression(&mut self) -> Result<Expr> {
        let mut left = self.term()?;
        while let Some(op) = match self.peek() {
            Some(Token::Plus) => Some(BinaryOp::Add),
            Some(Token::Minus) => Some(BinaryOp::Sub),
            _ => None,
        } {
            self.position += 1;
            let right = self.term()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Expr> {
        let mut left = self.unary()?;
        while let Some(op) = match self.peek() {
            Some(Token::Star) => Some(BinaryOp::Mul),
            Some(Token::Slash) => Some(BinaryOp::Div),
            _ => None,
        } {
            self.position += 1;
            let right = self.unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    // Unary minus binds looser than `^`, so `-x^2` is `-(x^2)`.
    fn unary(&mut self) -> Result<Expr> {
        match self.peek() {
            Some(Token::Minus) => {
                self.position += 1;
                Ok(Expr::Negate(Box::new(self.unary()?)))
            }
            Some(Token::Plus) => {
                self.position += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Expr> {
        let base = self.primary()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.position += 1;
            let exponent = self.unary()?;
            return Ok(Expr::Binary {
                op: BinaryOp::Pow,
                left: Box::new(base),
                right: Box::new(exponent),
            });
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.next() {
            Some(Token::Number(value)) => Ok(Expr::Number(value)),
            Some(Token::LParen) => {
                let inner = self.expression()?;
                self.expect(&Token::RParen, "')'")?;
                Ok(inner)
            }
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.position += 1;
                    let callable = callable(&name)
                        .ok_or_else(|| error(format!("unknown function {name}()")))?;
                    if matches!(self.peek(), Some(Token::RParen)) {
                        return Err(error(format!("{name}() needs one argument")));
                    }
                    let argument = Box::new(self.expression()?);
                    if matches!(self.peek(), Some(Token::Comma)) {
                        return Err(error(format!("{name}() takes exactly one argument")));
                    }
                    self.expect(&Token::RParen, "')'")?;
                    return Ok(match callable {
                        Callable::Function(function) => Expr::Function { function, argument },
                        Callable::Aggregate(kind) => Expr::Aggregate { kind, argument },
                    });
                }
                let dataset = if matches!(self.peek(), Some(Token::LBracket)) {
                    self.position += 1;
                    let index = match self.next() {
                        Some(Token::Number(value))
                            if value >= 1.0 && value.fract() == 0.0 && value <= 1e6 =>
                        {
                            value as usize
                        }
                        _ => {
                            return Err(error(format!(
                                "{name}[n] needs a dataset number starting at 1"
                            )));
                        }
                    };
                    self.expect(&Token::RBracket, "']'")?;
                    Some(index)
                } else {
                    None
                };
                Ok(Expr::Variable(VarRef { name, dataset }))
            }
            Some(token) => Err(error(format!("unexpected {token:?}"))),
            None => Err(error("formula ended unexpectedly")),
        }
    }
}

/// Parse a formula expression (without a `name =` prefix).
pub fn parse(text: &str) -> Result<Expr> {
    let tokens = tokenize(text)?;
    if tokens.is_empty() {
        return Err(error("formula is empty"));
    }
    let mut parser = Parser {
        tokens,
        position: 0,
    };
    let expr = parser.expression()?;
    if let Some(token) = parser.peek() {
        return Err(error(format!("unexpected {token:?} after expression")));
    }
    Ok(expr)
}

/// Unique variable references in order of first appearance.
pub fn references(expr: &Expr) -> Vec<VarRef> {
    fn walk(expr: &Expr, output: &mut Vec<VarRef>) {
        match expr {
            Expr::Number(_) => {}
            Expr::Variable(reference) => {
                if !output.contains(reference) {
                    output.push(reference.clone());
                }
            }
            Expr::Negate(inner)
            | Expr::Function {
                argument: inner, ..
            }
            | Expr::Aggregate {
                argument: inner, ..
            } => walk(inner, output),
            Expr::Binary { left, right, .. } => {
                walk(left, output);
                walk(right, output);
            }
        }
    }
    let mut output = Vec::new();
    walk(expr, &mut output);
    output
}

/// Whether the result varies along `axis`, i.e. some reference is not
/// enclosed by an aggregation over that axis.
pub fn depends_on(expr: &Expr, axis: AggregateAxis) -> bool {
    match expr {
        Expr::Number(_) => false,
        Expr::Variable(_) => true,
        Expr::Aggregate { kind, argument } => kind.axis() != axis && depends_on(argument, axis),
        Expr::Negate(inner)
        | Expr::Function {
            argument: inner, ..
        } => depends_on(inner, axis),
        Expr::Binary { left, right, .. } => depends_on(left, axis) || depends_on(right, axis),
    }
}

/// Data access used while evaluating a formula.
pub trait FormulaInputs {
    /// Read the 2-D plane of `reference` at the given time/layer indices.
    fn read(&self, reference: &VarRef, time: usize, depth: usize) -> Result<Slice2D>;
    /// Time and vertical-layer lengths of `reference`.
    fn lengths(&self, reference: &VarRef) -> Result<(usize, usize)>;
    fn is_cancelled(&self) -> bool {
        false
    }
}

struct Grid {
    values: Array2<f64>,
    missing: Array2<bool>,
    bounds: Bounds,
    coordinates: Option<Arc<CoordinateGrid>>,
}

enum Value {
    Scalar(f64),
    Grid(Grid),
}

/// Evaluate `expr` at the given time and layer indices into a displayable slice.
///
/// Missing or fill cells in any input stay missing; non-finite results from
/// the arithmetic itself (e.g. `log` of a negative value) are reported as
/// NaN/±Inf so domain errors remain visible.
pub fn evaluate(
    expr: &Expr,
    inputs: &dyn FormulaInputs,
    time: usize,
    depth: usize,
) -> Result<Slice2D> {
    match eval(expr, inputs, time, depth)? {
        Value::Scalar(_) => Err(error("formula must reference at least one variable")),
        Value::Grid(grid) => into_slice(grid),
    }
}

/// Evaluate a formula that contains only numbers and functions.
pub fn evaluate_constant(text: &str) -> Result<f64> {
    struct NoInputs;
    impl FormulaInputs for NoInputs {
        fn read(&self, reference: &VarRef, _: usize, _: usize) -> Result<Slice2D> {
            Err(error(format!("unknown variable {}", reference.name)))
        }
        fn lengths(&self, reference: &VarRef) -> Result<(usize, usize)> {
            Err(error(format!("unknown variable {}", reference.name)))
        }
    }
    match eval(&parse(text)?, &NoInputs, 0, 0)? {
        Value::Scalar(value) => Ok(value),
        Value::Grid(_) => Err(error("formula is not constant")),
    }
}

fn axis_index(index: usize, length: usize, axis: &str, reference: &VarRef) -> Result<usize> {
    if length <= 1 {
        Ok(0)
    } else if index < length {
        Ok(index)
    } else {
        Err(error(format!(
            "{} has {length} {axis} step(s); index {index} is out of range",
            reference.name
        )))
    }
}

fn eval(expr: &Expr, inputs: &dyn FormulaInputs, time: usize, depth: usize) -> Result<Value> {
    Ok(match expr {
        Expr::Number(value) => Value::Scalar(*value),
        Expr::Variable(reference) => {
            let (time_length, depth_length) = inputs.lengths(reference)?;
            let slice = inputs.read(
                reference,
                axis_index(time, time_length, "time", reference)?,
                axis_index(depth, depth_length, "layer", reference)?,
            )?;
            let missing = Zip::from(&slice.values)
                .and(&slice.validity)
                .map_collect(|value, validity| *validity != Validity::Finite || !value.is_finite());
            Value::Grid(Grid {
                values: slice.values,
                missing,
                bounds: slice.source_bounds,
                coordinates: slice.coordinates.clone(),
            })
        }
        Expr::Negate(inner) => map(eval(inner, inputs, time, depth)?, |value| -value),
        Expr::Function { function, argument } => {
            let function = *function;
            map(eval(argument, inputs, time, depth)?, move |value| {
                apply_function(function, value)
            })
        }
        Expr::Binary { op, left, right } => {
            let left = eval(left, inputs, time, depth)?;
            let right = eval(right, inputs, time, depth)?;
            let op = *op;
            combine(left, right, move |a, b| apply_binary(op, a, b))?
        }
        Expr::Aggregate { kind, argument } => aggregate(*kind, argument, inputs, time, depth)?,
    })
}

fn apply_function(function: Function, value: f64) -> f64 {
    match function {
        Function::Sin => value.sin(),
        Function::Cos => value.cos(),
        Function::Tan => value.tan(),
        Function::Log => value.ln(),
        Function::Log10 => value.log10(),
        Function::Exp => value.exp(),
        Function::Sqrt => value.sqrt(),
        Function::Abs => value.abs(),
    }
}

fn apply_binary(op: BinaryOp, left: f64, right: f64) -> f64 {
    match op {
        BinaryOp::Add => left + right,
        BinaryOp::Sub => left - right,
        BinaryOp::Mul => left * right,
        BinaryOp::Div => left / right,
        BinaryOp::Pow => left.powf(right),
    }
}

fn map(value: Value, function: impl Fn(f64) -> f64) -> Value {
    match value {
        Value::Scalar(value) => Value::Scalar(function(value)),
        Value::Grid(mut grid) => {
            grid.values.mapv_inplace(function);
            Value::Grid(grid)
        }
    }
}

fn combine(left: Value, right: Value, function: impl Fn(f64, f64) -> f64) -> Result<Value> {
    Ok(match (left, right) {
        (Value::Scalar(a), Value::Scalar(b)) => Value::Scalar(function(a, b)),
        (Value::Grid(mut grid), Value::Scalar(b)) => {
            grid.values.mapv_inplace(|a| function(a, b));
            Value::Grid(grid)
        }
        (Value::Scalar(a), Value::Grid(mut grid)) => {
            grid.values.mapv_inplace(|b| function(a, b));
            Value::Grid(grid)
        }
        (Value::Grid(mut left), Value::Grid(right)) => {
            if left.values.dim() != right.values.dim() {
                return Err(error(format!(
                    "grid shapes differ: {:?} vs {:?}",
                    left.values.dim(),
                    right.values.dim()
                )));
            }
            Zip::from(&mut left.values)
                .and(&right.values)
                .for_each(|a, &b| *a = function(*a, b));
            Zip::from(&mut left.missing)
                .and(&right.missing)
                .for_each(|a, &b| *a |= b);
            if left.coordinates.is_none() {
                left.coordinates = right.coordinates;
            }
            Value::Grid(left)
        }
    })
}

fn aggregate(
    kind: Aggregate,
    argument: &Expr,
    inputs: &dyn FormulaInputs,
    time: usize,
    depth: usize,
) -> Result<Value> {
    let axis = kind.axis();
    let mut steps = 1usize;
    for reference in references(argument) {
        let (time_length, depth_length) = inputs.lengths(&reference)?;
        let length = match axis {
            AggregateAxis::Time => time_length,
            AggregateAxis::Layer => depth_length,
        };
        if length > 1 {
            if steps > 1 && steps != length {
                return Err(error(format!(
                    "cannot aggregate references with {steps} and {length} steps together"
                )));
            }
            steps = length;
        }
    }
    let reduction = kind.reduction();
    let mut accumulated: Option<(Grid, Array2<usize>)> = None;
    for step in 0..steps {
        if inputs.is_cancelled() {
            return Err(NcvError::WorkerStopped);
        }
        let (step_time, step_depth) = match axis {
            AggregateAxis::Time => (step, depth),
            AggregateAxis::Layer => (time, step),
        };
        let grid = match eval(argument, inputs, step_time, step_depth)? {
            Value::Scalar(value) => return Ok(Value::Scalar(value)),
            Value::Grid(grid) => grid,
        };
        let (total, counts) = accumulated.get_or_insert_with(|| {
            let initial = match reduction {
                Reduction::Mean | Reduction::Sum => 0.0,
                Reduction::Min => f64::INFINITY,
                Reduction::Max => f64::NEG_INFINITY,
            };
            (
                Grid {
                    values: Array2::from_elem(grid.values.dim(), initial),
                    missing: Array2::from_elem(grid.values.dim(), false),
                    bounds: grid.bounds,
                    coordinates: grid.coordinates.clone(),
                },
                Array2::zeros(grid.values.dim()),
            )
        });
        if total.values.dim() != grid.values.dim() {
            return Err(error("grid shape changes between aggregated steps"));
        }
        Zip::from(&mut total.values)
            .and(counts)
            .and(&grid.values)
            .and(&grid.missing)
            .for_each(|accumulator, count, &value, &missing| {
                if missing {
                    return;
                }
                *count += 1;
                *accumulator = match reduction {
                    Reduction::Mean | Reduction::Sum => *accumulator + value,
                    // f64::min/max would silently drop NaN; keep it visible.
                    _ if value.is_nan() || accumulator.is_nan() => f64::NAN,
                    Reduction::Min => accumulator.min(value),
                    Reduction::Max => accumulator.max(value),
                };
            });
    }
    let Some((mut total, counts)) = accumulated else {
        return Err(error("aggregation has no steps"));
    };
    Zip::from(&mut total.values)
        .and(&mut total.missing)
        .and(&counts)
        .for_each(|value, missing, &count| {
            if count == 0 {
                *missing = true;
            } else if reduction == Reduction::Mean {
                *value /= count as f64;
            }
        });
    Ok(Value::Grid(total))
}

fn into_slice(grid: Grid) -> Result<Slice2D> {
    let validity = Zip::from(&grid.values)
        .and(&grid.missing)
        .map_collect(|value, missing| {
            if *missing {
                Validity::Missing
            } else if value.is_finite() {
                Validity::Finite
            } else if value.is_nan() {
                Validity::NaN
            } else if *value > 0.0 {
                Validity::PosInf
            } else {
                Validity::NegInf
            }
        });
    let mut values = grid.values;
    Zip::from(&mut values)
        .and(&grid.missing)
        .for_each(|value, missing| {
            if *missing {
                *value = f64::NAN;
            }
        });
    let slice = Slice2D::new(values, validity, grid.bounds)?;
    Ok(match grid.coordinates {
        Some(coordinates) => slice.with_shared_coordinates(coordinates),
        None => slice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed {
        frames: Vec<Array2<f64>>,
    }

    impl FormulaInputs for Fixed {
        fn read(&self, _: &VarRef, time: usize, _: usize) -> Result<Slice2D> {
            let values = self.frames[time].clone();
            let validity = values.mapv(|value| {
                if value.is_finite() {
                    Validity::Finite
                } else {
                    Validity::Missing
                }
            });
            Slice2D::new(values, validity, Bounds::new(0, 1, 0, 2)?)
        }

        fn lengths(&self, _: &VarRef) -> Result<(usize, usize)> {
            Ok((self.frames.len(), 1))
        }
    }

    #[test]
    fn aggregation_skips_missing_samples_and_flags_empty_cells() {
        let inputs = Fixed {
            frames: vec![
                Array2::from_shape_vec((1, 2), vec![1.0, f64::NAN]).unwrap(),
                Array2::from_shape_vec((1, 2), vec![3.0, f64::NAN]).unwrap(),
            ],
        };
        let slice = evaluate(&parse("mean(x)").unwrap(), &inputs, 0, 0).unwrap();
        assert_eq!(slice.values[(0, 0)], 2.0);
        assert_eq!(slice.validity[(0, 1)], Validity::Missing);
    }

    #[test]
    fn missing_inputs_propagate_through_arithmetic() {
        let inputs = Fixed {
            frames: vec![Array2::from_shape_vec((1, 2), vec![4.0, f64::NAN]).unwrap()],
        };
        let slice = evaluate(&parse("sqrt(x) + 1").unwrap(), &inputs, 0, 0).unwrap();
        assert_eq!(slice.values[(0, 0)], 3.0);
        assert_eq!(slice.validity[(0, 1)], Validity::Missing);
    }

    #[test]
    fn dependency_tracks_aggregated_axes() {
        let expr = parse("mean(x) - x").unwrap();
        assert!(depends_on(&expr, AggregateAxis::Time));
        let expr = parse("mean(x) * 2").unwrap();
        assert!(!depends_on(&expr, AggregateAxis::Time));
        assert!(depends_on(&expr, AggregateAxis::Layer));
    }

    #[test]
    fn quoted_names_allow_unusual_characters() {
        let expr = parse("\"air-temp\"[2] * 2").unwrap();
        assert_eq!(
            references(&expr),
            vec![VarRef {
                name: "air-temp".into(),
                dataset: Some(2)
            }]
        );
    }
}
