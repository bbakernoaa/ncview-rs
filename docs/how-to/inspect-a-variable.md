---
type: howto
category: how_to
tags: [variables, navigation, status-bar, curvilinear, subgroups]
---

# Inspect a variable

Goal: read exact values, understand coordinates, and inspect a specific point
of a dataset slice.

## Open and select files

```bash
ncv data.nc                      # single file
ncv run*.nc                      # shell glob (expanded by ncv itself too)
ncv a.nc b.nc c.nc               # explicit list; { and } switch files
```

Source files are opened **read-only** and never modified.

## Move through variables and slices

| Do this | Press |
| --- | --- |
| Previous / next variable | `↑` / `↓` (or click the sidebar list) |
| Previous / next time slice | `←` / `→`, or `<` / `>` |
| Previous / next depth slice | `[` / `]` (or the level bar) |
| Search all plottable variables | `/` |
| Run any action by name | `Ctrl-P` or `:` |

## Read the status bar

The status line reports, for the **displayed slice**:

- the active variable name (subgroup variables appear qualified, e.g.
  `physics/temperature`),
- **min / max / mean** and a **finite count** — non-finite values are counted
  but never averaged into the statistics,
- the active time label and depth label when the axes exist.

Hover the mouse anywhere on the map to see that cell's **latitude, longitude,
and source value**. On files without recognizable coordinate variables the
readout falls back to source indices — it says so explicitly rather than
inventing coordinates.

## Pin a point and inspect its history

1. **Click** a map cell — it gets a `◆` marker.
2. Press `Enter` to open its time-series plot, or `p` for the plot chooser:
   `t` time series, `d` scatter, `h` histogram, `k` CDF, `u` vertical profile.
3. Hover other points and press `m` to add them to the same plot for
   multi-trace comparison.

## Curvilinear and projected grids

Files with 2-D latitude/longitude fields render in projected mode. Press `g`
to switch between logical and projected grid. In projected mode the pixel →
coordinate lookup is nearest-coordinate; the status bar **discloses the
approximation** and keeps the original source indices visible. See
[Data fidelity](../explanation/data-fidelity.md) for what this does and does
not guarantee.

## Related

- [Style the map](style-the-map.md) — palettes, limits, masks
- [Keyboard reference](../reference/keyboard.md)
- [Supported formats](../reference/formats.md)
