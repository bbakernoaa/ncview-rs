---
type: cheatsheet
category: reference
tags: [cli, commands, flags, subcommands, cheatsheet]
---

# Command-line reference

`ncv [OPTIONS] [DATASET]... [COMMAND]`

## Options

| Flag | Values | Purpose |
| --- | --- | --- |
| `--diff` | — | Enable difference mode between two files or two sets of files |
| `--first <PATTERN>` | path/glob | First file or glob pattern for diff mode |
| `--second <PATTERN>` | path/glob | Second file or glob pattern for diff mode |
| `--formula <EXPR>` | expression | Add a derived variable (repeatable); `NAME[n]` reads dataset `n`. `-formula` is accepted as a VERDI-style alias |
| `--batch` | — | Evaluate every `--formula` and export PNG/SVG/JSON without opening the terminal UI |
| `--export-dir <DIR>` | path | Output directory for `--batch` (default `NCVIEW_EXPORT_DIR` or `.`) |
| `--time <INDEX>` | integer | Zero-based time step exported by `--batch` (default 0) |
| `--level <INDEX>` | integer | Zero-based vertical level exported by `--batch` (default 0) |
| `--no-restore` | — | Do not restore previous session state for the dataset(s) |
| `--min-x <X>` | number | Minimum x coordinate or zero-based x index for the initial view |
| `--max-x <X>` | number | Maximum x coordinate or zero-based x index for the initial view |
| `--min-y <Y>` | number | Minimum y coordinate or zero-based y index for the initial view |
| `--max-y <Y>` | number | Maximum y coordinate or zero-based y index for the initial view |
| `-h`, `--help` | — | Print help |
| `-V`, `--version` | — | Print version |

## Positional arguments

`[DATASET]...` — one or more NetCDF-4 or GRIB2 datasets to inspect. Shell
globs are supported (expanded by `ncv` itself, so quoting patterns works on
any shell). With `--diff`, inputs are split into two sets: use `first=`/`1=`
and `second=`/`2=` prefixes, or `--first`/`--second`, or simply list the first
set followed by the second.

## Subcommand: `ncv manifest`

Create a Kerchunk-compatible GRIB2 reference manifest from a `.idx` sidecar.

| Flag | Values | Purpose |
| --- | --- | --- |
| `--format` | `kerchunk` \| `virtualizarr` | Output profile; `virtualizarr` emits a VirtualiZarr-consumable Kerchunk profile |
| `--input` | path/URI | GRIB2 source object |
| `--idx` | path | Matching NOAA-style `.idx` sidecar |
| `--output` | path | Manifest destination JSON |
| `--source-uri` | URI | URI to place in byte-range references instead of the local source path |
| `--strict` | — | Treat warnings and mismatches as errors |

```bash
ncv manifest --format kerchunk --input forecast.grib2 --idx forecast.grib2.idx \
  --output forecast.refs.json --strict
```

## Examples

```bash
ncv data.nc                                  # open one file
ncv run*.grib2                               # open a collection
ncv --diff control.nc experiment.nc          # two-file difference mode
ncv --diff first="a*.nc" second="b*.nc"      # set-vs-set difference mode
ncv --no-restore data.nc                     # ignore saved session state
ncv s3://noaa-gefs-pds/...f000.grib2         # remote public bucket
ncv --min-x -130 --max-x -60 --min-y 20 --max-y 55 data.nc
ncv --min-x 0 --max-x 100 --min-y 20 --max-y 80 image-only.nc
ncv --formula "d = O3[1]-O3[2]" a.nc b.nc    # cross-file formula in the UI
ncv --batch --export-dir out --formula "mean(O3)" a.nc  # headless export
```

Supply all four bounds together. Geographic axes use coordinate values; longitude
input accepts either signed degrees (`-180..180`) or `0..360`, and latitude
must be between `-90` and `90`. The viewer maps the inclusive endpoints to the
smallest source-index rectangle containing those coordinate samples. Other axes
use their one-dimensional coordinate values when available, and inclusive,
zero-based cell indices when no coordinate values are available. Reversed,
non-finite, out-of-domain, or non-overlapping bounds produce an error.

Explicit CLI bounds take precedence over a zoom restored from the saved session.
Without CLI bounds, the usual restored view is preserved. Use `r` or the
command palette's **Reset zoom** action to return to the selected variable's
global view. For curvilinear grids the requested geographic rectangle is
approximated by its smallest row/column envelope; the status line reports this.
See [Derive variables with formulas](../how-to/derive-variables-with-formulas.md)
for the formula language.

## Related

- [Environment reference](environment.md)
- [Keyboard reference](keyboard.md)
- [How to generate a manifest](../how-to/generate-grib2-manifest.md)
