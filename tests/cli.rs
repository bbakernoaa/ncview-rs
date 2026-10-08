use std::process::Command;

fn ncv() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ncv"))
}

#[test]
fn help_and_version_identify_ncv() {
    let help = ncv().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("ncv"));
    let version = ncv().arg("--version").output().unwrap();
    assert!(version.status.success());
    // Compare against the package version rather than a literal so release
    // bumps never require editing this test.
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("ncv {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn missing_dataset_prints_usage_without_entering_terminal() {
    let output = ncv().output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: ncv"));
}

#[test]
fn manifest_subcommand_describes_both_profiles() {
    let output = ncv().args(["manifest", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(help.contains("kerchunk"));
    assert!(help.contains("virtualizarr"));
}

#[test]
fn grid_flag_appears_in_help() {
    let output = ncv().arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(help.contains("--grid"));
}

#[test]
fn bounded_view_options_are_all_required_before_terminal_startup() {
    let options = [
        "--min-x", "1", "--max-x", "2", "--min-y", "3", "--max-y", "4",
    ];
    for present_mask in 1_u8..15 {
        let arguments = (0..4)
            .filter(|index| present_mask & (1 << index) != 0)
            .flat_map(|index| [options[index * 2], options[index * 2 + 1]])
            .collect::<Vec<_>>();
        let output = ncv().args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("must be supplied together"));
    }
}

#[test]
fn bounded_view_help_documents_all_four_bounds() {
    let output = ncv().arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    for option in ["--min-x", "--max-x", "--min-y", "--max-y"] {
        assert!(help.contains(option), "missing {option} in help");
    }
}

#[test]
fn non_finite_bounded_view_values_fail_before_terminal_startup() {
    let output = ncv()
        .args([
            "--min-x", "NaN", "--max-x", "2", "--min-y", "3", "--max-y", "4",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("finite"));
}

#[test]
fn malformed_bounded_view_values_are_rejected_by_the_argument_parser() {
    let output = ncv()
        .args([
            "--min-x", "west", "--max-x", "2", "--min-y", "3", "--max-y", "4",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid value"));
}
