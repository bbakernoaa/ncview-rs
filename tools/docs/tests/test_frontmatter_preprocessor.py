#!/usr/bin/env python3
"""RED tests for tools/docs/frontmatter_preprocessor.py (task T005, TDD).

Run: python3 -m unittest discover tools/docs/tests
The preprocessor must strip leading YAML front-matter from chapter content and
substitute {{NCV_VERSION}} with the version parsed from Cargo.toml, speaking the
mdBook preprocessor JSON protocol on stdin/stdout.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOLS_DIR = HERE.parent  # tools/docs
SCRIPT = TOOLS_DIR / "frontmatter_preprocessor.py"


def load_module():
    spec = importlib.util.spec_from_file_location("frontmatter_preprocessor", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def chapter(name: str, path: str, content: str) -> dict:
    return {
        "Chapter": {
            "name": name,
            "content": content,
            "number": None,
            "key": path,
            "source_path": path,
            "path": path.replace(".md", ".html"),
            "draft": False,
            "sub_items": [],
        }
    }


def run_protocol(book: dict, cwd: Path, ctx: dict | None = None) -> tuple[int, object]:
    # mdBook 0.5.4 request: [PreprocessorContext, Book]; the context carries the
    # book root as "root". Response: the Book object alone on stdout.
    default_ctx = {"root": str(cwd), "config": {}, "renderer": "html", "mdbook_version": "0.5.4"}
    proc = subprocess.run(
        [sys.executable, str(SCRIPT)],
        input=json.dumps([ctx or default_ctx, book]),
        capture_output=True,
        text=True,
        cwd=cwd,
    )
    payload = json.loads(proc.stdout) if proc.stdout.strip() else None
    return proc.returncode, payload


class UnitStripTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def test_strip_leading_front_matter(self):
        text = "---\ntype: tutorial\ncategory: tutorials\ntags: [quickstart]\n---\n\n# Title\nbody\n"
        stripped = self.fp.strip_front_matter(text, "a.md")
        self.assertEqual(stripped, "\n# Title\nbody\n")

    def test_no_front_matter_passthrough(self):
        text = "# Just markdown\n\n- list\n"
        self.assertEqual(self.fp.strip_front_matter(text, "a.md"), text)

    def test_malformed_unterminated_block_raises(self):
        text = "---\ntype: tutorial\nno closing fence\n"
        with self.assertRaises(self.fp.FrontMatterError):
            self.fp.strip_front_matter(text, "a.md")

    def test_front_matter_only_file_becomes_empty(self):
        text = "---\ntype: project\ncategory: reference\ntags: [x]\n---\n"
        self.assertEqual(self.fp.strip_front_matter(text, "a.md"), "")

    def test_version_substitution(self):
        text = "ncv {{NCV_VERSION}} rocks {{NCV_VERSION}}"
        self.assertEqual(self.fp.substitute_version(text, "0.5.5"), "ncv 0.5.5 rocks 0.5.5")

    def test_idempotent_double_application(self):
        text = "---\ntype: howto\ncategory: how_to\ntags: [x]\n---\n\n# v {{NCV_VERSION}}\n"
        once = self.fp.substitute_version(self.fp.strip_front_matter(text, "a.md"), "0.5.5")
        twice = self.fp.substitute_version(self.fp.strip_front_matter(once, "a.md"), "0.5.5")
        self.assertEqual(once, twice)
        self.assertIn("0.5.5", once)


class CargoVersionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fp = load_module()

    def test_reads_package_version(self):
        text = '[package]\nname = "x"\nversion = "1.2.3"\nedition = "2024"\n'
        self.assertEqual(self.fp.parse_cargo_version(text), "1.2.3")

    def test_ignores_non_package_sections(self):
        text = '[package]\nname = "x"\n\n[[bin]]\nname = "y"\nversion = "9.9.9"\n'
        self.assertIsNone(self.fp.parse_cargo_version(text))

    def test_missing_version_returns_none(self):
        self.assertIsNone(self.fp.parse_cargo_version('[package]\nname = "x"\n'))


class ProtocolTests(unittest.TestCase):
    def fixture_root(self, tmp: Path) -> Path:
        (tmp / "Cargo.toml").write_text('[package]\nname = "ncview-rs"\nversion = "9.9.9"\n')
        return tmp

    def test_full_book_processing(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.fixture_root(Path(td))
            book = {
                "items": [
                    chapter(
                        "Quickstart",
                        "tutorials/quickstart.md",
                        "---\ntype: tutorial\ncategory: tutorials\ntags: [q]\n---\n\n# v {{NCV_VERSION}}\n",
                    ),
                    {"Separator": {}},
                    chapter("Plain", "plain.md", "# No front matter\n"),
                ]
            }
            code, payload = run_protocol(book, root)
            self.assertEqual(code, 0, payload)
            # mdBook parses stdout as the Book object itself, not an envelope.
            self.assertEqual(sorted(payload), ["items"])
            items = payload["items"]
            self.assertEqual(items[0]["Chapter"]["content"], "\n# v 9.9.9\n")
            self.assertEqual(items[2]["Chapter"]["content"], "# No front matter\n")

    def test_nested_sub_items_processed(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.fixture_root(Path(td))
            inner = chapter(
                "Inner",
                "reference/inner.md",
                "---\ntype: reference\ncategory: reference\ntags: [i]\n---\ninner {{NCV_VERSION}}",
            )
            outer = chapter(
                "Outer",
                "reference/index.md",
                "---\ntype: reference\ncategory: reference\ntags: [o]\n---\nouter",
            )
            outer["Chapter"]["sub_items"] = [inner]
            code, payload = run_protocol({"items": [outer]}, root)
            self.assertEqual(code, 0, payload)
            sub = payload["items"][0]["Chapter"]["sub_items"][0]
            self.assertEqual(sub["Chapter"]["content"], "inner 9.9.9")

    def test_cargo_found_via_parent_walk(self):
        # mdBook runs preprocessors with cwd at the book root (docs/) and passes
        # context["root"] = docs/; the repo Cargo.toml lives one level up.
        with tempfile.TemporaryDirectory() as td:
            book_root = Path(td) / "docs"
            book_root.mkdir()
            (Path(td) / "Cargo.toml").write_text('[package]\nversion = "8.8.8"\n')
            code, payload = run_protocol(
                {"items": [chapter("A", "a.md", "{{NCV_VERSION}}")]},
                book_root,
                ctx={"root": str(book_root), "config": {}, "renderer": "html", "mdbook_version": "0.5.4"},
            )
            self.assertEqual(code, 0, payload)
            self.assertEqual(payload["items"][0]["Chapter"]["content"], "8.8.8")

    def test_context_root_used_over_cwd(self):
        # When context["root"] and cwd differ, the context root is authoritative.
        with tempfile.TemporaryDirectory() as td:
            real_root = Path(td) / "book"
            real_root.mkdir()
            other = Path(td) / "elsewhere"
            other.mkdir()
            (real_root / "Cargo.toml").write_text('[package]\nversion = "7.7.7"\n')
            (other / "Cargo.toml").write_text('[package]\nversion = "1.1.1"\n')
            code, payload = run_protocol(
                {"items": [chapter("A", "a.md", "{{NCV_VERSION}}")]},
                other,
                ctx={"root": str(real_root), "config": {}, "renderer": "html", "mdbook_version": "0.5.4"},
            )
            self.assertEqual(code, 0, payload)
            self.assertEqual(payload["items"][0]["Chapter"]["content"], "7.7.7")

    def test_missing_version_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            (root / "Cargo.toml").write_text('[package]\nname = "x"\n')
            code, _ = run_protocol({"items": [chapter("A", "a.md", "# a\n")]}, root)
            self.assertNotEqual(code, 0)

    def test_supports_probe(self):
        # mdBook 0.5 probes `<cmd> supports <renderer>` with an empty stdin before
        # piping the real request. html must be accepted (exit 0); other renderers
        # declined (non-zero) so mdBook skips the preprocessor for them.
        for renderer, expect_ok in (("html", True), ("markdown", False)):
            proc = subprocess.run(
                [sys.executable, str(SCRIPT), "supports", renderer],
                input="",
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode == 0, expect_ok, renderer)

    def test_malformed_front_matter_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as td:
            root = self.fixture_root(Path(td))
            code, _ = run_protocol(
                {"items": [chapter("A", "a.md", "---\ntype: broken\n")]},
                root,
            )
            self.assertNotEqual(code, 0)


if __name__ == "__main__":
    unittest.main()
