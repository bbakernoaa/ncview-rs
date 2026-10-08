#!/usr/bin/env python3
"""Diátaxis docs validator for the ncv mdBook (FR-013 gate).

Validates ``docs/`` content against the contract in
``specs/006-mdbook-docs-site/contracts/diataxis-metadata.md``:

- ``frontmatter``     — M1/M2 schema on every authored page
- ``summary-parity``  — SUMMARY.md links and tracked pages match exactly
- ``links``           — relative links resolve; externals and ../api/ allowed
- ``drift-keyboard``  — keyboard reference matches ``src/ui/help.rs``   (Phase 4)
- ``drift-cli``       — CLI reference matches clap surface in ``src/main.rs`` (Phase 4)
- ``drift-env``       — env reference matches ``NCVIEW_*`` reads in ``src/`` (Phase 4)
- ``version``         — no literal versions; ``{{NCV_VERSION}}`` in index.md
- ``mermaid``         — fences balanced, known diagram types, safe aliases

Usage: ``validate_docs.py [REPO_ROOT]``. Exit codes: 0 all pass, 1 a check
failed, 2 tool error (missing docs root, unreadable file). Standard library only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# M2/M3: type -> (category, allowed directories relative to docs root).
# The `project` type may live at the docs root (landing page) or in reference/.
TYPE_TABLE: dict[str, tuple[str, tuple[str, ...]]] = {
    "tutorial": ("tutorials", ("tutorials",)),
    "howto": ("how_to", ("how-to",)),
    "explanation": ("explanation", ("explanation",)),
    "pr": ("explanation", ("explanation",)),
    "fivewhy": ("explanation", ("explanation",)),
    "reference": ("reference", ("reference",)),
    "adr": ("reference", ("reference",)),
    "cheatsheet": ("reference", ("reference",)),
    "project": ("reference", ("reference", "")),
    "wow": ("reference", ("reference",)),
}

# M4/CHK-SP: quadrant display order in SUMMARY.md (top-level section directories).
QUADRANT_ORDER = ("tutorials", "how-to", "reference", "explanation")

# Files under these docs-relative prefixes are not validated or required in SUMMARY.
EXEMPT_DIRS = ("superpowers/",)

TAG_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
# A semver-looking literal, not embedded in a longer dotted number (IPv4
# addresses such as 169.254.169.254 must not trip the version drift guard).
VERSION_LITERAL_RE = re.compile(r"(?<![\d.])v?\d+\.\d+\.\d+(?![\d.])")
MD_LINK_RE = re.compile(r"\]\(([^)\s]+)\)")
ADR_NAME_RE = re.compile(r"^\d{4}-")

MERMAID_TYPES = {
    "graph",
    "sequenceDiagram",
    "flowchart",
    "classDiagram",
    "stateDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "quadrantChart",
    "requirementDiagram",
    "gitGraph",
    "mindmap",
    "timeline",
}


class ToolError(Exception):
    """A condition that makes validation impossible (exit 2)."""


def docs_root(repo: Path) -> Path:
    """Resolve the docs root, honouring ``paths.docs_root`` from the project config."""
    config = repo / ".specify" / "extensions" / "spec-kit-diataxis-docs" / "diataxis-config.yml"
    if config.is_file():
        for line in config.read_text(encoding="utf-8").splitlines():
            match = re.match(r"\s+docs_root:\s*(\S+)", line)
            if match:
                value = match.group(1).strip("'\"")
                return (repo / value.rstrip("/")).resolve()
    return repo / "docs"


def is_exempt(path: Path, root: Path) -> bool:
    rel = path.relative_to(root).as_posix()
    return rel == "SUMMARY.md" or any(rel.startswith(d) for d in EXEMPT_DIRS)


def authored_pages(root: Path) -> list[Path]:
    if not root.is_dir():
        raise ToolError(f"docs root not found: {root}")
    pages = [p for p in sorted(root.rglob("*.md")) if p != root and not is_exempt(p, root)]
    return pages


def parse_front_matter(text: str) -> tuple[dict[str, str] | None, str]:
    """Return (fields, body). ``fields`` is None when no valid block exists.

    Recognises the flat ``key: value`` YAML subset the schema requires; nested
    structures are intentionally unsupported (M1).
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return None, text
    fields: dict[str, str] = {}
    for index, line in enumerate(lines[1:], start=1):
        stripped = line.strip()
        if stripped in ("---", "..."):
            body = "\n".join(lines[index + 1 :])
            return fields, body
        if not stripped or stripped.startswith("#"):
            continue
        key, sep, value = line.partition(":")
        if not sep:
            return None, text
        fields[key.strip()] = value.strip()
    return None, text  # unterminated block


# --------------------------------------------------------------------------- checks


def check_frontmatter(repo: Path) -> list[str]:
    root = docs_root(repo)
    issues: list[str] = []
    for page in authored_pages(root):
        rel = page.relative_to(root).as_posix()
        if page.name == "SUMMARY.md":
            continue
        text = page.read_text(encoding="utf-8")
        fields, _ = parse_front_matter(text)
        if fields is None:
            issues.append(f"{rel}: missing or malformed front-matter block")
            continue
        dtype = fields.get("type", "")
        category = fields.get("category", "")
        if dtype not in TYPE_TABLE:
            issues.append(f"{rel}: unknown type '{dtype}'")
            continue
        expected_category, allowed_dirs = TYPE_TABLE[dtype]
        if category != expected_category:
            issues.append(f"{rel}: type '{dtype}' requires category '{expected_category}', got '{category}'")
        parent = page.parent.relative_to(root).as_posix()
        if parent == ".":
            parent = ""
        if parent not in allowed_dirs:
            issues.append(f"{rel}: type '{dtype}' must live under docs/{allowed_dirs[0] or 'root'}")
        if dtype == "adr" and not ADR_NAME_RE.match(page.name):
            issues.append(f"{rel}: adr pages must be named NNNN-slug.md")
        tags = fields.get("tags", "")
        match = re.match(r"^\[(.*)\]$", tags)
        if not match:
            issues.append(f"{rel}: tags must be a flow list, e.g. [alpha, beta]")
            continue
        items = [t.strip() for t in match.group(1).split(",") if t.strip()]
        if not items:
            issues.append(f"{rel}: tags must be non-empty")
        for tag in items:
            if not TAG_RE.match(tag):
                issues.append(f"{rel}: tag '{tag}' is not lowercase-kebab")
    return issues


def check_summary_parity(repo: Path) -> list[str]:
    root = docs_root(repo)
    issues: list[str] = []
    summary = root / "SUMMARY.md"
    if not summary.is_file():
        return [f"{summary}: SUMMARY.md not found"]
    text = summary.read_text(encoding="utf-8")
    targets: list[str] = []
    for raw in MD_LINK_RE.findall(text):
        path = raw.split("#", 1)[0]
        if path:
            targets.append(path)

    seen: dict[str, int] = {}
    for target in targets:
        seen[target] = seen.get(target, 0) + 1
        resolved = (root / target).resolve()
        try:
            resolved.relative_to(root.resolve())
        except ValueError:
            issues.append(f"SUMMARY.md: link '{target}' escapes docs root")
            continue
        if not resolved.is_file():
            issues.append(f"SUMMARY.md: dangling link '{target}'")

    tracked = {p.relative_to(root).as_posix() for p in authored_pages(root)}
    for orphan in sorted(tracked - set(seen)):
        issues.append(f"SUMMARY.md: page not listed: {orphan}")
    for target, count in seen.items():
        if count > 1:
            issues.append(f"SUMMARY.md: '{target}' linked {count} times, expected exactly once")

    quadrants = []
    for target in targets:
        top = target.split("/", 1)[0] if "/" in target else ""
        if top and top not in quadrants:
            quadrants.append(top)
    ordered = [q for q in QUADRANT_ORDER if q in quadrants]
    if quadrants != ordered:
        issues.append(f"SUMMARY.md: quadrant order {quadrants} != {list(QUADRANT_ORDER)}")
    return issues


def check_links(repo: Path) -> list[str]:
    root = docs_root(repo)
    issues: list[str] = []
    for page in authored_pages(root):
        rel = page.relative_to(root).as_posix()
        text = page.read_text(encoding="utf-8")
        for raw in MD_LINK_RE.findall(text):
            target = raw.split("#", 1)[0]
            if not target or target.startswith(("http://", "https://", "mailto:")):
                continue
            if target == "../api/index.html":
                continue  # generated rustdoc landing page, present in the artifact
            resolved = (page.parent / target).resolve()
            try:
                resolved.relative_to(root.resolve())
            except ValueError:
                issues.append(f"{rel}: link '{raw}' escapes docs root")
                continue
            if not resolved.is_file():
                issues.append(f"{rel}: broken link '{raw}'")
    return issues


def check_version(repo: Path) -> list[str]:
    root = docs_root(repo)
    issues: list[str] = []
    for page in authored_pages(root):
        rel = page.relative_to(root).as_posix()
        text = page.read_text(encoding="utf-8")
        _, body = parse_front_matter(text)
        if rel == "index.md" and "{{NCV_VERSION}}" not in body:
            issues.append("index.md: must contain the {{NCV_VERSION}} token")
        for match in VERSION_LITERAL_RE.finditer(body):
            line = body[: match.start()].count("\n") + 1
            issues.append(f"{rel}:{line}: literal version '{match.group(0)}' — use {{{{NCV_VERSION}}}}")
    return issues


def check_mermaid(repo: Path) -> list[str]:
    root = docs_root(repo)
    issues: list[str] = []
    fence_re = re.compile(r"^\s*```\s*mermaid\s*$")
    close_re = re.compile(r"^\s*```\s*$")
    for page in authored_pages(root):
        rel = page.relative_to(root).as_posix()
        lines = page.read_text(encoding="utf-8").splitlines()
        index = 0
        while index < len(lines):
            if not fence_re.match(lines[index]):
                index += 1
                continue
            start = index + 1
            end = None
            for j in range(start, len(lines)):
                if close_re.match(lines[j]):
                    end = j
                    break
            if end is None:
                issues.append(f"{rel}:{index + 1}: unbalanced mermaid fence")
                break
            block = lines[start:end]
            first = next((ln.strip() for ln in block if ln.strip()), "")
            keyword = first.split()[0] if first else ""
            if keyword not in MERMAID_TYPES:
                issues.append(f"{rel}:{start + 1}: unknown mermaid diagram type '{keyword}'")
            for offset, ln in enumerate(block):
                match = re.match(r"\s*(?:participant|actor)\s+\S+\s+as\s+(.+?)\s*$", ln)
                if match and any(ch in match.group(1) for ch in ('(', ')', '"', "'")):
                    issues.append(
                        f"{rel}:{start + offset + 1}: participant alias contains unsafe characters"
                    )
            index = end + 1
    return issues


# ------------------------------------------------------------------ drift guards
#
# Code is the source of truth; the reference tables must mirror it exactly
# (SC-002). Each guard parses the real surface and the doc table, then compares.


def _norm(text: str) -> str:
    """Lowercase, drop backticks, and collapse whitespace for token comparison."""
    return re.sub(r"\s+", " ", text.replace("`", "").strip()).lower()


def extract_help_text(help_rs: Path) -> str:
    """Reconstruct the runtime string returned by ``help_text()``.

    Walks the string literal from its opening quote to the matching (unescaped)
    closing quote, then resolves Rust line-continuations (``\\`` + newline +
    next-line indent) and the ``\\n`` escapes, so the parser sees exactly the text
    the help popup renders. A body with no string literal (e.g. ``String::new()``)
    yields the empty string.
    """
    src = help_rs.read_text(encoding="utf-8")
    header = re.search(r"(?:pub )?fn help_text\(\)\s*->\s*String\s*\{", src)
    if not header:
        raise ToolError(f"drift-keyboard: help_text() not found in {help_rs}")
    rest = src[header.end() :]
    open_idx = rest.find('"')
    if open_idx == -1:
        return ""
    chars: list[str] = []
    i = open_idx + 1
    while i < len(rest):
        c = rest[i]
        if c == "\\":
            chars.append(rest[i : i + 2])
            i += 2
            continue
        if c == '"':
            break
        chars.append(c)
        i += 1
    raw = "".join(chars)
    raw = re.sub(r"\\[ \t]*\n[ \t]*", "", raw)  # line continuations
    raw = raw.replace("\\n", "\n").replace("\\t", "\t").replace('\\"', '"').replace("\\\\", "\\")
    return raw


def help_binding_tokens(text: str) -> set[str]:
    """Leading tokens of aligned help lines (token + 2+ spaces + description).

    Section headers (``Keyboard``/``Mouse``/``Command palette``) and prose lines
    have no alignment column, so they are not treated as bindings.
    """
    tokens: set[str] = set()
    for line in text.splitlines():
        match = re.match(r"(\S.*?)\s{2,}\S", line)
        if match:
            tokens.add(_norm(match.group(1)))
    return tokens


def doc_table_first_cells(md_path: Path) -> list[str]:
    """First-column cell of every markdown table data row (headers/separators skipped)."""
    lines = md_path.read_text(encoding="utf-8").splitlines()
    cells: list[str] = []
    for idx, line in enumerate(lines):
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        row = stripped.strip("|").split("|")
        if all(re.match(r"\s*:?-{3,}:?\s*$", c) for c in row):
            continue  # separator row
        nxt = lines[idx + 1].strip() if idx + 1 < len(lines) else ""
        if nxt.startswith("|"):
            nrow = nxt.strip("|").split("|")
            if nrow and all(re.match(r"\s*:?-{3,}:?\s*$", c) for c in nrow):
                continue  # header row (a separator follows it)
        cells.append(_norm(row[0]))
    return cells


def check_drift_keyboard(repo: Path) -> list[str]:
    root = docs_root(repo)
    help_rs = repo / "src" / "ui" / "help.rs"
    keyboard = root / "reference" / "keyboard.md"
    if not help_rs.is_file():
        return [f"drift-keyboard: source of truth missing: {help_rs}"]
    if not keyboard.is_file():
        return [f"drift-keyboard: docs page missing: {keyboard}"]

    text = extract_help_text(help_rs)
    help_norm = _norm(text)
    help_tokens = help_binding_tokens(text)
    doc_tokens = doc_table_first_cells(keyboard)

    issues: list[str] = []
    # Rule A: every documented row's primary token appears verbatim in help text.
    for token in doc_tokens:
        if token and token not in help_norm:
            issues.append(f"keyboard.md: documented control '{token}' is not in help_text()")
    # Rule B: every help binding token is documented (catches new bindings).
    documented = set(doc_tokens)
    for token in sorted(help_tokens):
        if token not in documented:
            issues.append(f"keyboard.md: help_text() binding '{token}' is undocumented")
    return issues


def _clap_subcommand_names(main_src: str) -> set[str]:
    """Variants of the ``#[derive(Subcommand)]`` enum, lowercased."""
    names: set[str] = set()
    for m in re.finditer(r"enum\s+(\w+)", main_src):
        # Confirm this enum is the subcommand enum by checking the derive above it.
        prefix = main_src[: m.start()]
        if "#[derive" not in prefix:
            continue
        derive = prefix[prefix.rindex("#[derive") :]
        if "Subcommand" not in derive:
            continue
        brace = main_src.index("{", m.end())
        depth = 0
        end = brace
        for i in range(brace, len(main_src)):
            if main_src[i] == "{":
                depth += 1
            elif main_src[i] == "}":
                depth -= 1
                if depth == 0:
                    end = i
                    break
        body = main_src[brace + 1 : end]
        for v in re.finditer(r"^\s*(?:///[^\n]*\n\s*)*(\w+)\s*[\{,]", body, re.M):
            names.add(v.group(1).lower())
    return names


def parse_clap_flags(main_src: str) -> tuple[set[str], set[str]]:
    """Return (flag set, subcommand set) derived from clap attributes.

    If the file has no clap `#[arg(...)]`/`#[derive(Parser)]` surface at all, the
    flag set is empty — a fixture or entrypoint that predates clap is treated as
    "no documented CLI surface to enforce", not as a hard `--help` requirement.
    """
    has_clap = "#[arg(" in main_src or "derive(Parser)" in main_src
    flags: set[str] = set()
    for m in re.finditer(r"#\[arg\([^)]*\blong\b[^)]*\)\]\s*(?:pub\s+)?(\w+)\s*:", main_src):
        flags.add("--" + m.group(1).replace("_", "-"))
    if has_clap:
        # clap always adds --help; --version only when #[command(version)] is set.
        flags.add("--help")
        if re.search(r"#\[command\([^)]*\bversion\b", main_src, re.S):
            flags.add("--version")
    return flags, _clap_subcommand_names(main_src)


def check_drift_cli(repo: Path) -> list[str]:
    root = docs_root(repo)
    main_rs = repo / "src" / "main.rs"
    cli = root / "reference" / "cli.md"
    if not main_rs.is_file():
        return [f"drift-cli: source of truth missing: {main_rs}"]
    if not cli.is_file():
        return [f"drift-cli: docs page missing: {cli}"]

    main_src = main_rs.read_text(encoding="utf-8")
    actual_flags, actual_subs = parse_clap_flags(main_src)
    doc_text = cli.read_text(encoding="utf-8")
    doc_flags = set(re.findall(r"--([a-z][a-z0-9]+(?:-[a-z0-9]+)*)", doc_text))
    doc_flags = {"--" + f for f in doc_flags}
    doc_subs = {m.lower() for m in re.findall(r"Subcommand:\s*`ncv\s+([a-z][a-z0-9-]+)`", doc_text)}

    issues: list[str] = []
    for flag in sorted(doc_flags - actual_flags):
        issues.append(f"cli.md: documented flag '{flag}' is not in the clap surface")
    for flag in sorted(actual_flags - doc_flags):
        issues.append(f"cli.md: clap flag '{flag}' is undocumented")
    for sub in sorted(doc_subs - actual_subs):
        issues.append(f"cli.md: documented subcommand '{sub}' is not in the clap surface")
    for sub in sorted(actual_subs - doc_subs):
        issues.append(f"cli.md: clap subcommand '{sub}' is undocumented")
    return issues


ENV_TOKEN_RE = re.compile(r"NCVIEW_[A-Z][A-Z0-9_]*")
ENV_ALLOWLIST = {"NCVIEWBASE", "RAYON_NUM_THREADS"}


def check_drift_env(repo: Path) -> list[str]:
    root = docs_root(repo)
    env = root / "reference" / "environment.md"
    src = repo / "src"
    if not src.is_dir():
        return [f"drift-env: source tree missing: {src}"]
    if not env.is_file():
        return [f"drift-env: docs page missing: {env}"]

    actual: set[str] = set()
    for path in sorted(src.rglob("*.rs")):
        actual.update(ENV_TOKEN_RE.findall(path.read_text(encoding="utf-8")))
    documented = set(ENV_TOKEN_RE.findall(env.read_text(encoding="utf-8")))

    issues: list[str] = []
    for token in sorted(documented - actual - ENV_ALLOWLIST):
        issues.append(f"environment.md: documented '{token}' is not read anywhere in src/")
    for token in sorted(actual - documented):
        issues.append(f"environment.md: src reads '{token}' but it is undocumented")
    return issues


CHECKS = [
    ("frontmatter", check_frontmatter),
    ("summary-parity", check_summary_parity),
    ("links", check_links),
    ("drift-keyboard", check_drift_keyboard),
    ("drift-cli", check_drift_cli),
    ("drift-env", check_drift_env),
    ("version", check_version),
    ("mermaid", check_mermaid),
]


def main(argv: list[str]) -> int:
    repo = Path(argv[1]).resolve() if len(argv) > 1 else Path.cwd()
    try:
        failures = 0
        for name, check in CHECKS:
            issues = check(repo)
            if issues:
                failures += 1
                print(f"FAIL {name}")
                for issue in issues:
                    print(f"  - {issue}")
            else:
                print(f"PASS {name}")
        return 1 if failures else 0
    except ToolError as error:
        print(f"tool error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
