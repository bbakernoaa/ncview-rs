#!/usr/bin/env python3
"""Reject raw GRIB sidecars and files too large for normal Git hosting."""

from __future__ import annotations

import sys
from pathlib import Path


BLOCKED_SUFFIXES = (".grib", ".grib2", ".grb", ".grb2", ".idx")
MAX_FILE_SIZE = 100 * 1024 * 1024


def main(paths: list[str]) -> int:
    violations: list[str] = []
    for raw_path in paths:
        path = Path(raw_path)
        # A deleted staged path is valid and has no file to inspect.
        if not path.exists():
            continue
        lowered = path.name.lower()
        if lowered.endswith(BLOCKED_SUFFIXES):
            violations.append(f"{path}: raw GRIB/IDX dataset files are not committed")
            continue
        try:
            size = path.stat().st_size
        except OSError as error:
            violations.append(f"{path}: cannot inspect file size: {error}")
            continue
        if size >= MAX_FILE_SIZE:
            violations.append(
                f"{path}: {size / (1024 * 1024):.1f} MiB exceeds the 100 MiB GitHub limit"
            )

    if violations:
        print("pre-commit rejected the staged files:", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
