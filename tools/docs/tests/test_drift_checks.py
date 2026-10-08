#!/usr/bin/env python3
"""RED drift-guard tests for validate_docs.py (task T016, TDD).

Run: python3 -m unittest discover tools/docs/tests

These cover the three drift checks (contracts/diataxis-metadata.md §M4 CHK-KB/CL/EN)
using synthetic fixtures: a mini `help_text()` literal, mini clap `#[arg(long)]`
lines, a mini env-scan source tree, and matching/mismatching reference tables.
The stub checks in validate_docs.py currently return no issues, so the mismatch
assertions below are RED until T017 implements the real parsing.
"""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE.parent / "validate_docs.py"


def load_module():
    spec = importlib.util.spec_from_file_location("validate_docs", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


FM = "---\ntype: {t}\ncategory: reference\ntags: [drift]\n---\n\n"


def write(path: Path, text: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def make_repo(tmp: str, *, help_text: str, main_rs: str, extra_src: dict[str, str],
              keyboard: str, cli: str, environment: str) -> Path:
    root = Path(tmp)
    (root / "Cargo.toml").write_text('[package]\nname = "ncview-rs"\nversion = "0.5.5"\n')
    write(root / "src" / "ui" / "help.rs", help_text)
    write(root / "src" / "main.rs", main_rs)
    for rel, content in extra_src.items():
        write(root / "src" / rel, content)
    ref = root / "docs" / "reference"
    write(ref / "keyboard.md", FM.format(t="cheatsheet") + keyboard)
    write(ref / "cli.md", FM.format(t="cheatsheet") + cli)
    write(ref / "environment.md", FM.format(t="reference") + environment)
    return root


HELP_OK = (
    "pub fn help_text() -> String {\n"
    '    "Keyboard\\n\\\n'
    "  q / Esc       quit\\n\\\n"
    "  w             new binding\\n\\\n"
    "  Shift+arrows   pan the map\\n\\\n"
    'Mouse\\n\\\n'
    "  move over map  hover readout\\n\\\n"
    "  right-click or ? closes help\\n\\\n"
    '  Ctrl-P or :   palette"\n'
    "        .to_string()\n"
    "}\n"
)

# keyboard.md table whose primary tokens exactly cover the help bindings above.
KEYBOARD_OK = (
    "# Keyboard & mouse reference\n\n"
    "## Keyboard\n\n| Key | Action |\n| --- | --- |\n"
    "| `q` / `Esc` | quit |\n| `w` | new binding |\n| `Shift`+arrows | pan the map |\n\n"
    "## Mouse\n\n| Gesture | Action |\n| --- | --- |\n"
    "| Move over map | hover readout |\n| Right-click or `?` | closes help |\n"
    "| `Ctrl-P` or `:` | palette |\n\n## Related\n\n- [CLI](cli.md)\n"
)

MAIN_OK = (
    "use clap::{Parser, Subcommand};\n"
    "#[derive(Parser)]\n#[command(version)]\nstruct Cli {\n"
    "    #[arg(long)]\n    alpha: bool,\n"
    "    #[arg(long)]\n    beta_two: bool,\n"
    "    #[arg(value_name = \"X\", num_args = 0..)]\n    pos: Vec<String>,\n}\n"
    "#[derive(Subcommand)]\nenum CliCommand {\n"
    "    Gamma {\n        #[arg(long)]\n        delta: bool,\n    },\n}\n"
)

CLI_OK = (
    "# Command-line reference\n\n## Options\n\n| Flag | Values | Purpose |\n| --- | --- | --- |\n"
    "| `--alpha` | — | a |\n| `--beta-two` | — | b |\n| `--help` | — | h |\n| `--version` | — | v |\n\n"
    "## Subcommand: `ncv gamma`\n\n| Flag | Values | Purpose |\n| --- | --- | --- |\n"
    "| `--delta` | — | d |\n\n## Related\n\n- [Env](environment.md)\n"
)

ENV_SRC = (
    "pub fn run() {\n"
    '    let _ = std::env::var("NCVIEW_ALPHA");\n'
    '    let _ = std::env::var("NCVIEW_BETA");\n'
    "}\n"
)

ENVIRONMENT_OK = (
    "# Environment reference\n\n| Variable | Values | Purpose |\n| --- | --- | --- |\n"
    "| `NCVIEW_ALPHA` | int | a |\n| `NCVIEW_BETA` | int | b |\n"
    "| `NCVIEWBASE` | dir | alias |\n| `RAYON_NUM_THREADS` | int | alias |\n\n"
    "## Related\n\n- [CLI](cli.md)\n"
)


class DriftKeyboardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def test_matching_tables_pass(self):
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=ENVIRONMENT_OK)
            self.assertEqual(self.fp.check_drift_keyboard(root), [])

    def test_undocumented_help_binding_fails(self):
        # help has binding `w`, keyboard.md omits it.
        missing_w = KEYBOARD_OK.replace("| `w` | new binding |\n", "")
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={},
                             keyboard=missing_w, cli=CLI_OK, environment=ENVIRONMENT_OK)
            issues = self.fp.check_drift_keyboard(root)
            self.assertTrue(any("w" in i for i in issues), issues)

    def test_stale_documented_binding_fails(self):
        # keyboard.md documents `z` but help has no `z` binding line.
        stale = KEYBOARD_OK + "\n| `z` | stale |\n"
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={},
                             keyboard=stale, cli=CLI_OK, environment=ENVIRONMENT_OK)
            issues = self.fp.check_drift_keyboard(root)
            self.assertTrue(any("z" in i for i in issues), issues)


class DriftCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def test_matching_flags_pass(self):
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=ENVIRONMENT_OK)
            self.assertEqual(self.fp.check_drift_cli(root), [])

    def test_nonexistent_documented_flag_fails(self):
        bad = CLI_OK.replace("| `--delta` | — | d |\n", "| `--delta` | — | d |\n| `--nonexistent-flag` | — | x |\n")
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={},
                             keyboard=KEYBOARD_OK, cli=bad, environment=ENVIRONMENT_OK)
            issues = self.fp.check_drift_cli(root)
            self.assertTrue(any("nonexistent-flag" in i for i in issues), issues)

    def test_undocumented_real_flag_fails(self):
        # main.rs adds --gamma-real but cli.md never mentions it.
        main = MAIN_OK.replace("    #[arg(long)]\n    beta_two: bool,\n",
                               "    #[arg(long)]\n    beta_two: bool,\n    #[arg(long)]\n    gamma_real: bool,\n")
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=main, extra_src={},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=ENVIRONMENT_OK)
            issues = self.fp.check_drift_cli(root)
            self.assertTrue(any("gamma-real" in i for i in issues), issues)


class DriftEnvTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def test_matching_env_passes(self):
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={"env.rs": ENV_SRC},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=ENVIRONMENT_OK)
            self.assertEqual(self.fp.check_drift_env(root), [])

    def test_missing_documented_env_fails(self):
        # environment.md documents NCVIEW_MYSTERY that src never reads.
        bad = ENVIRONMENT_OK.replace("| `NCVIEWBASE`", "| `NCVIEW_MYSTERY` | x | m |\n| `NCVIEWBASE`")
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={"env.rs": ENV_SRC},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=bad)
            issues = self.fp.check_drift_env(root)
            self.assertTrue(any("NCVIEW_MYSTERY" in i for i in issues), issues)

    def test_extra_undocumented_env_fails(self):
        # src reads NCVIEW_GAMMA that environment.md omits.
        src = ENV_SRC.replace('NCVIEW_BETA', 'NCVIEW_BETA_OR_GAMMA') + "\npub const G: &str = \"NCVIEW_GAMMA\";\n"
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={"env.rs": src},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=ENVIRONMENT_OK)
            issues = self.fp.check_drift_env(root)
            self.assertTrue(any("NCVIEW_GAMMA" in i for i in issues), issues)

    def test_allowlisted_aliases_pass(self):
        # NCVIEWBASE and RAYON_NUM_THREADS are documented but not NCVIEW_ scan hits.
        env = ENV_SRC + "\npub const R: &str = \"RAYON_NUM_THREADS\";\n"
        with tempfile.TemporaryDirectory() as td:
            root = make_repo(td, help_text=HELP_OK, main_rs=MAIN_OK, extra_src={"env.rs": env},
                             keyboard=KEYBOARD_OK, cli=CLI_OK, environment=ENVIRONMENT_OK)
            self.assertEqual(self.fp.check_drift_env(root), [])


class RealRepoDriftTests(unittest.TestCase):
    """The real docs must be in sync with the real code surfaces (T017 target)."""

    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()
        cls.repo = Path(__file__).resolve().parents[3]

    def test_real_keyboard_in_sync(self):
        self.assertEqual(self.fp.check_drift_keyboard(self.repo), [])

    def test_real_cli_in_sync(self):
        self.assertEqual(self.fp.check_drift_cli(self.repo), [])

    def test_real_env_in_sync(self):
        self.assertEqual(self.fp.check_drift_env(self.repo), [])


if __name__ == "__main__":
    unittest.main()
