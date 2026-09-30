#!/usr/bin/env python3
"""RED tests for tools/docs/validate_docs.py — the five Phase-2 base checks (T007, TDD).

Run: python3 -m unittest discover tools/docs/tests

Covered checks (specs/006-mdbook-docs-site/contracts/diataxis-metadata.md §M4):
frontmatter, summary-parity, links, version, mermaid. The three drift checks
(drift-keyboard/cli/env) are covered by test_drift_checks.py in Phase 4.

Exit-code contract (§C5): 0 pass, 1 check failure, 2 tool error.
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
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


GOOD_FM = "---\ntype: {type}\ncategory: {category}\ntags: [alpha, beta]\n---\n\n# {title}\n"

TYPE_TO_CATEGORY = {
    "tutorial": "tutorials",
    "howto": "how_to",
    "explanation": "explanation",
    "reference": "reference",
    "cheatsheet": "reference",
    "project": "reference",
}


class FixtureMixin:
    def make_repo(self, root: Path, pages: dict[str, str], summary: str, cargo_version: str = "0.5.5"):
        docs = root / "docs"
        docs.mkdir(parents=True, exist_ok=True)
        for rel, content in pages.items():
            path = docs / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        (docs / "SUMMARY.md").write_text(summary)
        (root / "Cargo.toml").write_text(f'[package]\nname = "ncview-rs"\nversion = "{cargo_version}"\n')
        src = root / "src"
        src.mkdir(exist_ok=True)
        (src / "main.rs").write_text("fn main() {}\n")
        return root


class FrontMatterCheckTests(unittest.TestCase, FixtureMixin):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def run_check(self, root: Path):
        return self.fp.check_frontmatter(root)

    def test_valid_pages_pass(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(
                Path(td),
                {
                    "index.md": GOOD_FM.format(type="project", category="reference", title="Intro"),
                    "tutorials/quickstart.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q"),
                },
                "# Summary\n\n- [Intro](index.md)\n- [Q](tutorials/quickstart.md)\n",
            )
            self.assertEqual(self.run_check(root), [])

    def test_missing_block_fails(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), {"index.md": "# No front matter\n"}, "# Summary\n\n- [I](index.md)\n")
            self.assertTrue(self.run_check(root))

    def test_unknown_type_fails(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(
                Path(td),
                {"index.md": "---\ntype: bogus\ncategory: reference\ntags: [x]\n---\n# I\n"},
                "# Summary\n\n- [I](index.md)\n",
            )
            self.assertTrue(self.run_check(root))

    def test_type_category_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(
                Path(td),
                {"tutorials/q.md": "---\ntype: tutorial\ncategory: reference\ntags: [x]\n---\n# Q\n"},
                "# Summary\n\n- [Q](tutorials/q.md)\n",
            )
            self.assertTrue(self.run_check(root))

    def test_empty_tags_fails(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(
                Path(td),
                {"index.md": "---\ntype: project\ncategory: reference\ntags: []\n---\n# I\n"},
                "# Summary\n\n- [I](index.md)\n",
            )
            self.assertTrue(self.run_check(root))

    def test_wrong_directory_for_category_fails(self):
        # tutorial category is `tutorials`; placing it under how-to/ is a mismatch.
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(
                Path(td),
                {"how-to/q.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q")},
                "# Summary\n\n- [Q](how-to/q.md)\n",
            )
            self.assertTrue(self.run_check(root))


class SummaryParityTests(unittest.TestCase, FixtureMixin):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def summary_ok(self):
        return "# Summary\n\n- [I](index.md)\n- [Q](tutorials/quickstart.md)\n"

    def pages_ok(self):
        return {
            "index.md": GOOD_FM.format(type="project", category="reference", title="I"),
            "tutorials/quickstart.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q"),
        }

    def test_dangling_link_fails(self):
        with tempfile.TemporaryDirectory() as td:
            summary = "# Summary\n\n- [I](index.md)\n- [Q](tutorials/missing.md)\n"
            root = self.make_repo(Path(td), self.pages_ok(), summary)
            self.assertTrue(self.fp.check_summary_parity(root))

    def test_orphan_page_fails(self):
        with tempfile.TemporaryDirectory() as td:
            pages = dict(self.pages_ok())
            pages["how-to/orphan.md"] = GOOD_FM.format(type="howto", category="how_to", title="O")
            root = self.make_repo(Path(td), pages, self.summary_ok())
            self.assertTrue(self.fp.check_summary_parity(root))

    def test_duplicate_link_fails(self):
        with tempfile.TemporaryDirectory() as td:
            summary = self.summary_ok() + "- [Q again](tutorials/quickstart.md)\n"
            root = self.make_repo(Path(td), self.pages_ok(), summary)
            self.assertTrue(self.fp.check_summary_parity(root))

    def test_clean_tree_passes(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), self.pages_ok(), self.summary_ok())
            self.assertEqual(self.fp.check_summary_parity(root), [])


class LinksCheckTests(unittest.TestCase, FixtureMixin):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def test_broken_relative_link_fails(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {
                "index.md": GOOD_FM.format(type="project", category="reference", title="I")
                + "\n[bad](does-not-exist.md)\n",
                "tutorials/quickstart.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q"),
            }
            root = self.make_repo(
                Path(td), pages, "# Summary\n\n- [I](index.md)\n- [Q](tutorials/quickstart.md)\n"
            )
            self.assertTrue(self.fp.check_links(root))

    def test_healthy_and_external_and_api_links_pass(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {
                "index.md": GOOD_FM.format(type="project", category="reference", title="I")
                + "\n[ok](tutorials/quickstart.md) [ext](https://example.com/x) [api](../api/index.html)\n",
                "tutorials/quickstart.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q"),
            }
            root = self.make_repo(
                Path(td), pages, "# Summary\n\n- [I](index.md)\n- [Q](tutorials/quickstart.md)\n"
            )
            self.assertEqual(self.fp.check_links(root), [])

    def test_anchor_only_link_checks_file(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {
                "index.md": GOOD_FM.format(type="project", category="reference", title="I")
                + "\n[see](tutorials/quickstart.md#some-heading)\n",
                "tutorials/quickstart.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q"),
            }
            root = self.make_repo(
                Path(td), pages, "# Summary\n\n- [I](index.md)\n- [Q](tutorials/quickstart.md)\n"
            )
            self.assertEqual(self.fp.check_links(root), [])


class VersionCheckTests(unittest.TestCase, FixtureMixin):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def summary(self):
        return "# Summary\n\n- [I](index.md)\n"

    def test_literal_version_fails(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {"index.md": GOOD_FM.format(type="project", category="reference", title="I") + "\nncv 0.5.5 is out\n"}
            root = self.make_repo(Path(td), pages, self.summary())
            self.assertTrue(self.fp.check_version(root))

    def test_token_in_index_passes(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {"index.md": GOOD_FM.format(type="project", category="reference", title="I") + "\nDocs for ncv {{NCV_VERSION}}\n"}
            root = self.make_repo(Path(td), pages, self.summary())
            self.assertEqual(self.fp.check_version(root), [])

    def test_missing_token_in_index_fails(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {"index.md": GOOD_FM.format(type="project", category="reference", title="I") + "\nno token here\n"}
            root = self.make_repo(Path(td), pages, self.summary())
            self.assertTrue(self.fp.check_version(root))

    def test_ip_address_is_not_a_version(self):
        with tempfile.TemporaryDirectory() as td:
            pages = {
                "index.md": GOOD_FM.format(type="project", category="reference", title="I")
                + "\nDocs for ncv {{NCV_VERSION}} via 169.254.169.254\n"
            }
            root = self.make_repo(Path(td), pages, self.summary())
            self.assertEqual(self.fp.check_version(root), [])


class MermaidCheckTests(unittest.TestCase, FixtureMixin):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def summary(self):
        return "# Summary\n\n- [I](index.md)\n"

    def page(self, body: str):
        return GOOD_FM.format(type="project", category="reference", title="I") + body

    def test_valid_diagram_passes(self):
        body = "\n```mermaid\nsequenceDiagram\n    autonumber\n    participant UI as Terminal UI\n    UI->>UI: ping\n```\n"
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), {"index.md": self.page(body)}, self.summary())
            self.assertEqual(self.fp.check_mermaid(root), [])

    def test_unbalanced_fence_fails(self):
        body = "\n```mermaid\ngraph TD\n    A-->B\n"
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), {"index.md": self.page(body)}, self.summary())
            self.assertTrue(self.fp.check_mermaid(root))

    def test_unknown_diagram_type_fails(self):
        body = "\n```mermaid\nnotAType XYZ\n```\n"
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), {"index.md": self.page(body)}, self.summary())
            self.assertTrue(self.fp.check_mermaid(root))

    def test_unsafe_participant_chars_fails(self):
        body = (
            "\n```mermaid\nsequenceDiagram\n    participant Py as Python (xarray)\n"
            "    Py->>Py: run\n```\n"
        )
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), {"index.md": self.page(body)}, self.summary())
            self.assertTrue(self.fp.check_mermaid(root))


def full_valid_pages() -> dict[str, str]:
    """A fixture tree satisfying every check, including the Phase-4 drift sources."""
    return {
        "index.md": GOOD_FM.format(type="project", category="reference", title="I")
        + "\nDocs for ncv {{NCV_VERSION}}\n",
        "tutorials/quickstart.md": GOOD_FM.format(type="tutorial", category="tutorials", title="Q"),
        "reference/keyboard.md": GOOD_FM.format(type="reference", category="reference", title="K"),
        "reference/cli.md": GOOD_FM.format(type="reference", category="reference", title="C"),
        "reference/environment.md": GOOD_FM.format(type="reference", category="reference", title="E"),
    }


def full_summary() -> str:
    return (
        "# Summary\n\n- [I](index.md)\n- [Q](tutorials/quickstart.md)\n"
        "- [K](reference/keyboard.md)\n- [C](reference/cli.md)\n- [E](reference/environment.md)\n"
    )


class CliContractTests(FixtureMixin, unittest.TestCase):
    """Exit-code contract from §C5 over synthetic trees (repo-state independent)."""

    def add_drift_sources(self, root: Path):
        help_rs = root / "src" / "ui" / "help.rs"
        help_rs.parent.mkdir(parents=True, exist_ok=True)
        help_rs.write_text("fn help_text() -> String { String::new() }\n")

    def test_valid_fixture_exits_0(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(Path(td), full_valid_pages(), full_summary())
            self.add_drift_sources(root)
            proc = subprocess.run([sys.executable, str(SCRIPT), str(root)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            for name in ("frontmatter", "summary-parity", "links", "version", "mermaid"):
                self.assertIn(f"PASS {name}", proc.stdout)

    def test_broken_fixture_exits_1(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.make_repo(
                Path(td),
                {"index.md": "# no front matter and a 0.5.5 literal\n"},
                "# Summary\n\n- [I](index.md)\n",
            )
            proc = subprocess.run([sys.executable, str(SCRIPT), str(root)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)

    def test_missing_docs_exits_2(self):
        with tempfile.TemporaryDirectory() as td:
            proc = subprocess.run([sys.executable, str(SCRIPT), td], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 2)


if __name__ == "__main__":
    unittest.main()
