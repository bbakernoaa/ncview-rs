use std::{path::PathBuf, process::Command, sync::Arc};

use ncview_rs::{
    analysis::formula::{self, Aggregate, Expr, VarRef},
    app::{AppState, Command as AppCommand, Overlay},
    data::{
        self, AxisRole, DataSource,
        formula::{FormulaDefinition, FormulaSource},
        slice::{Bounds, SliceRequest, Validity},
    },
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/coards-float32.nc4")
}

fn open_fixture() -> Arc<dyn DataSource> {
    Arc::from(data::open(fixture()).unwrap())
}

fn request(variable: &str, time: usize) -> SliceRequest {
    SliceRequest {
        variable: variable.into(),
        time,
        depth: 0,
        bounds: Bounds::new(0, 4, 0, 5).unwrap(),
    }
}

fn formula_source(expressions: &[&str], datasets: Vec<Arc<dyn DataSource>>) -> FormulaSource {
    let definitions = expressions
        .iter()
        .map(|text| FormulaDefinition::parse(text).unwrap())
        .collect::<Vec<_>>();
    FormulaSource::new(Arc::clone(&datasets[0]), datasets, &definitions).unwrap()
}

#[test]
fn parser_respects_operator_precedence_and_associativity() {
    let expr = formula::parse("1 + 2 * 3 ^ 2 ^ 0.5").unwrap();
    let Expr::Binary { .. } = expr else {
        panic!("expected binary expression");
    };
    assert_eq!(formula::evaluate_constant("1 + 2 * 3").unwrap(), 7.0);
    assert_eq!(formula::evaluate_constant("2 ^ 3 ^ 2").unwrap(), 512.0);
    assert_eq!(formula::evaluate_constant("2 ** 3").unwrap(), 8.0);
    assert_eq!(formula::evaluate_constant("-2 ^ 2").unwrap(), -4.0);
    assert_eq!(formula::evaluate_constant("(1 + 2) * 3").unwrap(), 9.0);
    assert_eq!(formula::evaluate_constant("10 / 4 - 0.5").unwrap(), 2.0);
    assert_eq!(formula::evaluate_constant("2 ^ -1").unwrap(), 0.5);
    assert_eq!(formula::evaluate_constant("1.5e2").unwrap(), 150.0);
}

#[test]
fn parser_supports_math_functions() {
    let close = |text: &str, expected: f64| {
        let value = formula::evaluate_constant(text).unwrap();
        assert!((value - expected).abs() < 1e-12, "{text} = {value}");
    };
    close("sin(0)", 0.0);
    close("cos(0)", 1.0);
    close("tan(0)", 0.0);
    close("log(exp(2))", 2.0);
    close("ln(exp(1))", 1.0);
    close("log10(1000)", 3.0);
    close("sqrt(16)", 4.0);
    close("abs(-3)", 3.0);
}

#[test]
fn parser_collects_dataset_qualified_references() {
    let expr = formula::parse("O3[1] - O3[2] + mean(NO2)").unwrap();
    let references = formula::references(&expr);
    assert_eq!(
        references,
        vec![
            VarRef {
                name: "O3".into(),
                dataset: Some(1)
            },
            VarRef {
                name: "O3".into(),
                dataset: Some(2)
            },
            VarRef {
                name: "NO2".into(),
                dataset: None
            },
        ]
    );
    let Expr::Binary { right, .. } = expr else {
        panic!("expected binary expression");
    };
    assert!(matches!(
        *right,
        Expr::Aggregate {
            kind: Aggregate::TimeMean,
            ..
        }
    ));
}

#[test]
fn parser_reports_helpful_errors() {
    for text in [
        "",
        "1 +",
        "(1 + 2",
        "foo(1)",
        "O3[0]",
        "sin(1, 2)",
        "1 $ 2",
        "mean()",
    ] {
        assert!(formula::parse(text).is_err(), "{text:?} should not parse");
    }
}

#[test]
fn named_definitions_split_on_assignment() {
    let named = FormulaDefinition::parse("total = MACCity + Pixel_area").unwrap();
    assert_eq!(named.name, "total");
    assert_eq!(named.expression, "MACCity + Pixel_area");
    let unnamed = FormulaDefinition::parse("  MACCity * 2 ").unwrap();
    assert_eq!(unnamed.name, "MACCity * 2");
    assert!(FormulaDefinition::parse("bad name = 1").is_err());
}

#[test]
fn formula_source_adds_derived_variables_and_evaluates_elementwise() {
    let source = formula_source(&["sum2 = MACCity + Pixel_area * 2"], vec![open_fixture()]);
    let variable = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "sum2")
        .unwrap();
    assert_eq!(variable.dimensions, vec!["date", "lat", "lon"]);
    assert!(variable.numeric);
    assert_eq!(
        variable.long_name.as_deref(),
        Some("MACCity + Pixel_area * 2")
    );

    let slice = source.read_slice(&request("sum2", 1)).unwrap();
    // time 1: MACCity = 21.., Pixel_area = 121..
    assert_eq!(slice.values[(0, 0)], 21.0 + 2.0 * 121.0);
    assert_eq!(slice.values[(3, 4)], 40.0 + 2.0 * 140.0);
    assert!(slice.validity.iter().all(|cell| *cell == Validity::Finite));

    // Raw variables still pass through untouched.
    let raw = source.read_slice(&request("MACCity", 2)).unwrap();
    assert_eq!(raw.values[(0, 0)], 41.0);
}

#[test]
fn time_aggregates_reduce_over_every_time_step() {
    let source = formula_source(
        &[
            "avg = mean(MACCity)",
            "total = sum(MACCity)",
            "lo = min(MACCity)",
            "hi = max(MACCity)",
        ],
        vec![open_fixture()],
    );
    let avg = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "avg")
        .unwrap();
    // The time axis is consumed by the aggregation.
    assert_eq!(avg.dimensions, vec!["lat", "lon"]);

    let value = |name: &str| source.read_slice(&request(name, 0)).unwrap().values[(0, 0)];
    assert_eq!(value("avg"), (1.0 + 21.0 + 41.0) / 3.0);
    assert_eq!(value("total"), 1.0 + 21.0 + 41.0);
    assert_eq!(value("lo"), 1.0);
    assert_eq!(value("hi"), 41.0);
}

#[test]
fn layer_aggregates_without_vertical_axis_use_the_single_level() {
    let source = formula_source(&["lm = layer_mean(MACCity)"], vec![open_fixture()]);
    let slice = source.read_slice(&request("lm", 2)).unwrap();
    assert_eq!(slice.values[(0, 0)], 41.0);
}

#[test]
fn cross_dataset_references_combine_multiple_files() {
    let first = open_fixture();
    let second = open_fixture();
    let source = formula_source(
        &["delta = MACCity[1] - MACCity[2]", "ratio = MACCity[2] / Pixel_area[1]"],
        vec![first, second],
    );
    let delta = source.read_slice(&request("delta", 0)).unwrap();
    assert!(delta.values.iter().all(|value| *value == 0.0));
    let ratio = source.read_slice(&request("ratio", 0)).unwrap();
    assert_eq!(ratio.values[(0, 0)], 1.0 / 101.0);
}

#[test]
fn invalid_references_are_rejected_when_building_the_source() {
    let base = open_fixture();
    let missing_variable = FormulaDefinition::parse("nope + 1").unwrap();
    assert!(
        FormulaSource::new(
            Arc::clone(&base),
            vec![Arc::clone(&base)],
            &[missing_variable]
        )
        .is_err()
    );
    let missing_dataset = FormulaDefinition::parse("MACCity[3]").unwrap();
    assert!(FormulaSource::new(Arc::clone(&base), vec![base], &[missing_dataset]).is_err());
}

#[test]
fn domain_errors_are_flagged_instead_of_hidden() {
    let source = formula_source(
        &["neg = log(MACCity - 10)", "inf = MACCity / 0"],
        vec![open_fixture()],
    );
    let neg = source.read_slice(&request("neg", 0)).unwrap();
    // MACCity(0,0)=1 -> log(-9) is NaN; MACCity(3,4)=20 -> log(10).
    assert_eq!(neg.validity[(0, 0)], Validity::NaN);
    assert_eq!(neg.validity[(3, 4)], Validity::Finite);
    assert!((neg.values[(3, 4)] - 10.0_f64.ln()).abs() < 1e-12);
    let inf = source.read_slice(&request("inf", 0)).unwrap();
    assert_eq!(inf.validity[(0, 0)], Validity::PosInf);
}

#[test]
fn time_labels_follow_the_first_referenced_variable() {
    let base = open_fixture();
    let source = formula_source(&["double = MACCity * 2"], vec![Arc::clone(&base)]);
    assert_eq!(
        source.time_label_for_variable("double", 1),
        base.time_label_for_variable("MACCity", 1)
    );
    assert!(
        source
            .metadata()
            .dimensions
            .iter()
            .any(|dimension| dimension.role == AxisRole::Time)
    );
}

#[test]
fn formula_editor_collects_and_submits_expressions() {
    let mut state = AppState::default();
    state.reduce(AppCommand::OpenFormulaEditor);
    assert_eq!(state.view.overlay, Some(Overlay::Formula));
    for character in "MACCity*2".chars() {
        state.reduce(AppCommand::InputChar(character));
    }
    state.reduce(AppCommand::DeleteInput);
    state.reduce(AppCommand::InputChar('3'));
    assert_eq!(state.view.formula_draft, "MACCity*3");
    state.reduce(AppCommand::SubmitFormula);
    assert_eq!(state.view.formulas, vec!["MACCity*3".to_string()]);
    assert_eq!(state.view.formula_request.as_deref(), Some("MACCity*3"));
    assert_eq!(state.view.overlay, None);

    // A parse error keeps the editor open and reports the problem.
    state.reduce(AppCommand::OpenFormulaEditor);
    for character in "1 +".chars() {
        state.reduce(AppCommand::InputChar(character));
    }
    state.view.formula_request = None;
    state.reduce(AppCommand::SubmitFormula);
    assert_eq!(state.view.overlay, Some(Overlay::Formula));
    assert!(state.view.status.contains("formula"));
    assert_eq!(state.view.formulas.len(), 1);
    assert!(state.view.formula_request.is_none());

    // Recalling and removing a stored formula.
    state.reduce(AppCommand::FormulaMove(-1));
    assert_eq!(state.view.formula_draft, "MACCity*3");
    state.reduce(AppCommand::RemoveFormula);
    assert!(state.view.formulas.is_empty());
    assert!(state.view.formulas_changed);
}

fn ncv() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ncv"))
}

#[test]
fn batch_formula_exports_images_without_a_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let output = ncv()
        .args(["--batch", "--export-dir"])
        .arg(directory.path())
        .args(["--formula", "avg = mean(MACCity)", "--time", "0"])
        .arg(fixture())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let names = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.ends_with("avg_t0000_z0000.png")), "{names:?}");
    assert!(names.iter().any(|name| name.ends_with("avg_t0000_z0000.svg")), "{names:?}");
    assert!(names.iter().any(|name| name.ends_with("avg_t0000_z0000.json")), "{names:?}");
}

#[test]
fn verdi_style_single_dash_formula_flag_is_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let first = fixture();
    let output = ncv()
        .args(["--batch", "--export-dir"])
        .arg(directory.path())
        .args(["-formula", "MACCity[1]-MACCity[2]"])
        .arg(&first)
        .arg(&first)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let count = std::fs::read_dir(directory.path())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".png")
        })
        .count();
    assert_eq!(count, 1);
}

#[test]
fn batch_reports_formula_errors() {
    let directory = tempfile::tempdir().unwrap();
    let output = ncv()
        .args(["--batch", "--export-dir"])
        .arg(directory.path())
        .args(["--formula", "missing_var * 2"])
        .arg(fixture())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing_var"));
}

#[test]
fn batch_requires_a_formula() {
    let output = ncv().arg("--batch").arg(fixture()).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--formula"));
}
