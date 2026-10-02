---
type: howto
category: how_to
tags: [formula, equation-editor, verdi, derived-variables, batch, multi-file]
---

# Derive variables with formulas

Goal: compute new fields from the variables you already have — differences
between runs, unit conversions, wind speed, time-mean maps — and view or export
them like any other variable. The feature follows the
[VERDI](https://www.cmascenter.org/verdi/) formula editor: every dataset you
open gets a number, and `NAME[n]` refers to variable `NAME` in dataset `n`.

## Open several datasets at once

List every file on the command line (globs work, quoted or not). The order
sets the dataset numbers used in formulas:

```bash
ncv base.nc sensitivity.nc          # [1] = base.nc, [2] = sensitivity.nc
ncv "run_*.nc"                      # numbered in sorted order
ncv s3://bucket/a.nc local/b.nc     # remote and local sources mix freely
```

The formula editor lists the numbered datasets at the top, so you never have
to remember the order.

## Use the formula editor

1. Press `=` (or open the command palette with `:` and choose
   **Open formula editor**).
2. Type an expression, for example `O3[1] - O3[2]`.
3. Press `Enter`. The result is added to the variable list and displayed
   immediately; time, depth, zoom, plots, and export (`e`) all work on it.

| Key | In the formula editor |
| --- | --- |
| any character | type into the formula |
| `Backspace` | delete the last character |
| `Enter` | add the formula (or replace the one with the same name) and plot it |
| `↑` / `↓` | recall a formula defined earlier into the input line |
| `Delete` | remove the recalled formula |
| `Esc` | close the editor (the draft is kept) |

Give a result a short name with `name = expression`; otherwise the expression
text itself is the variable name:

```text
dO3 = O3[1] - O3[2]
wspd = sqrt(U^2 + V^2)
T_C = T - 273.15
```

## Formula language

| Category | Syntax | Notes |
| --- | --- | --- |
| Arithmetic | `+` `-` `*` `/` | usual precedence; parentheses group |
| Exponent | `^` or `**` | right-associative: `2^3^2` = `2^9`; `-x^2` = `-(x^2)` |
| Trigonometry | `sin(x)` `cos(x)` `tan(x)` | radians |
| Logarithm / exponential | `log(x)` (natural; alias `ln`), `log10(x)`, `exp(x)` | |
| Other | `sqrt(x)`, `abs(x)` | |
| Time aggregation | `mean(x)` `sum(x)` `min(x)` `max(x)` | per grid cell, over every time step |
| Layer aggregation | `layer_mean(x)` `layer_sum(x)` `layer_min(x)` `layer_max(x)` | per grid cell, over every vertical level |
| Variables | `NAME` | the dataset currently on screen |
| Dataset variables | `NAME[n]` | dataset `n` (1-based) |
| Quoted names | `"air-temp"` or `'air temp'` | names with characters other than letters, digits, `_`, `.` |
| Numbers | `2`, `0.5`, `1.5e-3` | |

Aggregations can be combined with everything else, e.g. the anomaly from the
time mean is `T - mean(T)`, and the column-mean difference between two runs is
`layer_mean(O3[1]) - layer_mean(O3[2])`.

### How results are shaped

- The result takes the grid and dimensions of the **first variable** in the
  formula; all other grids must have the same 2-D shape.
- A time aggregation removes the time axis when nothing outside it varies in
  time (`mean(T)` is a single map; `T - mean(T)` still has every time step).
  Layer aggregations do the same for the vertical axis.
- A variable without a time (or vertical) axis is reused for every step, so a
  static field such as `area` can multiply a time-varying one.
- Variables whose time lengths differ (other than 1) are rejected rather than
  silently misaligned.

### Missing values and domain errors

- A cell that is missing, fill, or out of the valid range in **any** input is
  missing in the result.
- Aggregations skip missing samples; a cell with no valid sample is missing.
- Values produced by the arithmetic itself, like `log` of a negative number or
  division by zero, are kept as NaN / ±Inf and shown with the usual non-finite
  diagnostics, so domain errors are never hidden.

### Which dataset owns a formula

- A formula that uses any unqualified `NAME` is evaluated **for each dataset**:
  in a time collection, `T * 2` doubles every file.
- A formula in which every reference has `[n]` is attached only to the first
  referenced dataset, so it appears once in the timeline. Selecting it switches
  to that dataset automatically.

## Pass formulas on the command line

`--formula` (repeatable) preloads formulas and selects the first one. The
VERDI spelling `-formula` is accepted too:

```bash
ncv --formula "dO3 = O3[1] - O3[2]" base.nc sensitivity.nc
ncv -formula "mean(PM25)" -formula "max(PM25)" cmaq_*.nc
```

## Generate images without the terminal UI (batch mode)

Add `--batch` to evaluate every `--formula` and write the same PNG, SVG, and
JSON files as the `e` key, then exit — useful in scripts, cron jobs, and HPC
batch queues:

```bash
ncv --batch --export-dir plots \
    --formula "dO3 = O3[1] - O3[2]" \
    --formula "avg = mean(O3[1])" \
    --time 12 --level 0 \
    base.nc sensitivity.nc
```

| Flag | Default | Purpose |
| --- | --- | --- |
| `--batch` | off | export instead of opening the terminal UI |
| `--export-dir <DIR>` | `NCVIEW_EXPORT_DIR` or `.` | output directory |
| `--time <INDEX>` | `0` | zero-based time step to export |
| `--level <INDEX>` | `0` | zero-based vertical level to export |

Each written PNG path is printed on its own line. File names follow
`<dataset>_<formula>_t<time>_z<level>.{png,svg,json}`; the export environment
variables in [Compare and export](compare-and-export.md#tune-the-export) apply.
A parse error, an unknown variable, or an out-of-range index stops the run with
exit status 2 and a message naming the formula.

## Related

- [Compare and export](compare-and-export.md) — `--diff` mode and export options
- [Navigate time, depth, and files](navigate-time-depth-files.md)
- [Command-line reference](../reference/cli.md)
- [Keyboard reference](../reference/keyboard.md)
