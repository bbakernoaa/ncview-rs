#!/usr/bin/env python3
"""mdBook preprocessor: strip Diátaxis YAML front-matter and inject the version.

mdBook 0.5 does not parse YAML front-matter — it renders the block as visible
content. The Diátaxis protocol, however, requires ``type``/``category``/``tags``
metadata on every authored page (see
``specs/006-mdbook-docs-site/contracts/diataxis-metadata.md``). This preprocessor
bridges the two: authored files keep their front-matter for validators and audits,
while the rendered site receives clean Markdown.

It also substitutes the ``{{NCV_VERSION}}`` token with the ``version`` field of the
repository ``Cargo.toml`` (the release-plz-owned source of truth), so no version
literal is ever hand-maintained in the docs.

Protocol: mdBook passes ``[context, book]`` JSON on stdin; the preprocessor emits
``[context, book, true]`` on stdout (research.md R2, docs-pipeline contract C6).

Standard library only.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

VERSION_TOKEN = "{{NCV_VERSION}}"


class FrontMatterError(Exception):
    """Raised when a leading front-matter block is malformed."""


class PreprocessorError(Exception):
    """Raised when the version source (Cargo.toml) cannot be resolved."""


def strip_front_matter(text: str, name: str) -> str:
    """Remove a leading ``---\\n...\\n---\\n`` YAML block from *text*.

    Files without a leading fence are returned unchanged. An unterminated fence
    raises :class:`FrontMatterError` — a missing closing ``---`` is almost always
    a truncated edit, and silently rendering it as content would corrupt the page.
    """
    if not text.startswith("---"):
        return text
    # The opening fence must be alone on its line (allow an optional BOM-free
    # trailing space and CRLF).
    first_newline = text.find("\n")
    if first_newline == -1 or text[:first_newline].rstrip() != "---":
        # Not a front-matter block (e.g. a setext underline or "--- text"); leave
        # the content untouched.
        return text
    rest = text[first_newline + 1 :]
    close = _find_closing_fence(rest)
    if close is None:
        raise FrontMatterError(f"{name}: unterminated front-matter block (missing closing ---)")
    # rest[close:] begins at the closing fence line; drop through its newline.
    after = rest[close:]
    nl = after.find("\n")
    return "" if nl == -1 else after[nl + 1 :]


def _find_closing_fence(rest: str) -> int | None:
    """Index of the first line in *rest* that is exactly ``---`` or ``...``."""
    offset = 0
    for line in rest.splitlines(keepends=True):
        stripped = line.rstrip("\r\n")
        if stripped == "---" or stripped == "...":
            return offset
        offset += len(line)
    return None


def substitute_version(text: str, version: str) -> str:
    """Replace every ``{{NCV_VERSION}}`` token with *version*."""
    return text.replace(VERSION_TOKEN, version)


def parse_cargo_version(toml_text: str) -> str | None:
    """Return ``[package]`` ``version`` from *toml_text*, or ``None``.

    Minimal line scanner (no TOML dependency): only the exact ``[package]``
    section is considered, and the scan stops at the next section header so
    unrelated ``version =`` keys elsewhere are never picked up.
    """
    in_package = False
    for raw in toml_text.splitlines():
        line = raw.strip()
        if line.startswith("#"):
            continue
        if line.startswith("["):
            in_package = line == "[package]"
            continue
        if in_package and line.startswith("version"):
            _, _, value = line.partition("=")
            value = value.split("#", 1)[0].strip().strip('"').strip("'")
            if value:
                return value
    return None


def find_cargo_version(start: Path) -> str:
    """Walk up from *start* looking for a ``Cargo.toml`` with a resolvable version."""
    for directory in [start, *start.parents]:
        candidate = directory / "Cargo.toml"
        if candidate.is_file():
            version = parse_cargo_version(candidate.read_text(encoding="utf-8"))
            if version:
                return version
    raise PreprocessorError(
        f"could not find a [package] version in Cargo.toml at or above {start}"
    )


def process_item(item: dict, version: str) -> None:
    """Strip front-matter and substitute the version token, in place.

    mdBook's ``BookItem`` enum serialises as ``{"Chapter": {...}}``; separators
    and preformatted blocks carry no chapter content and pass through.
    """
    if "Chapter" in item:
        process_chapter(item["Chapter"], version)


def process_chapter(chapter: dict, version: str) -> None:
    content = chapter.get("content", "")
    name = chapter.get("source_path") or chapter.get("path") or chapter.get("name", "?")
    chapter["content"] = substitute_version(strip_front_matter(content, str(name)), version)
    for sub in chapter.get("sub_items", []):
        process_item(sub, version)


def main(argv: list[str]) -> int:
    # mdBook 0.5 probes preprocessors with `<command> supports <renderer>` and an
    # empty stdin before piping the real request. Exit 0 only for renderers this
    # preprocessor handles; any other renderer means "skip me".
    if len(argv) >= 3 and argv[1] == "supports":
        return 0 if argv[2] == "html" else 1

    try:
        raw = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        print(f"frontmatter preprocessor: invalid JSON on stdin: {error}", file=sys.stderr)
        return 2

    if not isinstance(raw, list) or len(raw) < 2:
        print("frontmatter preprocessor: expected [context, book] JSON", file=sys.stderr)
        return 2

    context, book = raw[0], raw[1]
    # mdBook runs preprocessors with cwd at the book root (docs/) and provides
    # the same path as context["root"]. Prefer the context; fall back to cwd.
    start = Path.cwd()
    if isinstance(context, dict) and context.get("root"):
        start = Path(context["root"])

    try:
        version = find_cargo_version(start)
    except PreprocessorError as error:
        print(f"frontmatter preprocessor: {error}", file=sys.stderr)
        return 2

    try:
        for item in book.get("items", []):
            process_item(item, version)
    except FrontMatterError as error:
        print(f"frontmatter preprocessor: {error}", file=sys.stderr)
        return 2

    # mdBook parses stdout as the Book object itself — not a response envelope.
    json.dump(book, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
