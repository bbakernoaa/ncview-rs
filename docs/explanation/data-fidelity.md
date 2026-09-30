---
type: explanation
category: explanation
tags: [fidelity, missing-values, fill, nan, infinity, coordinates, trust]
---

# Why the values can be trusted

`ncv` is a scientific viewer first and a visualization tool second. Its core
correctness rule is **source-value fidelity**: every displayed value,
coordinate, limit, axis, and index is traceable to the source dataset, and
presentation never silently alters data.

## Missing, fill, NaN, and infinity

- NetCDF `_FillValue` and missing-value attributes are applied by the data
  loader; masked cells are never painted as if they were data.
- Non-finite floating-point values (`NaN`, `+Inf`, `-Inf`) are **preserved** —
  they are never coerced to zero, dropped from counting, or averaged into
  statistics. The status bar's min/max/mean cover finite values only, and the
  finite count tells you how much of the slice was real data.
- The `f` data mask keeps only values between chosen bounds, but automatic
  fill/missing masking stays active regardless — masking is additive, never a
  way to make invalid values look valid.

## Coordinates and indices

- Coordinate variables are read from dataset metadata, including 2-D
  curvilinear latitude/longitude fields.
- When a file has no usable coordinate variable, the hover readout falls back
  to **source indices** and says so, rather than inventing plausible
  coordinates. Grouped (subgroup) coordinate fields currently use the same
  honest fallback while retaining the variable's data and metadata.
- Pinned-point time series and plots use the original source indices, so a
  click always identifies the exact source cell.

## Where approximation is allowed — and how it is disclosed

Rendering and projection MAY approximate spatial presentation. The places
they do are always labeled:

1. **Curvilinear lat/lon**: 2-D coordinate planes are read and used to report
   each cell's latitude/longitude, but pixels map to source cells by index, so
   a cell's value and readout are always the exact source cell. Nearest-lat/lon
   projection helpers (`ProjectionIndex`, `projected_lookup`) exist in the
   library but are not yet wired into the display; the `g` grid-mode toggle
   records a preference that no renderer consumes yet.
2. **Viewport-bounded aggregation**: when a slice is larger than the terminal
   canvas, source cells are reduced into display bins before color mapping.
   This bounds memory to the visible area — it changes how densely values are
   drawn, not their values. Nearest-neighbor is the default so scientific
   cells stay exact at any zoom; interpolation (`NCVIEW_SCIENTIFIC_RENDERING=0`
   plus a filter) is opt-in and clearly a display choice.

The coastline backdrop (`b`) is likewise presentation-only: it never changes
data values and never replaces a dataset-provided land/sea mask.

## Limits and scaling honesty

- Automatic limits follow the displayed slice (current-scope) or the full
  unzoomed range (global-scope) — the mode is visible, switchable (`z`), and
  both are computed from actual data.
- Manual limits always win over automatic modes.
- Log scaling is only applied when meaningful; the scale mode is reported in
  the UI.

## Read-only by construction

Datasets are opened read-only. Reference manifests (Kerchunk/VirtualiZarr)
describe byte ranges of the original files; opening one never copies or
mutates the source. Exports write new files and leave sources untouched.

## Related

- [Inspect a variable](../how-to/inspect-a-variable.md)
- [Supported formats](../reference/formats.md)
- [Remote access](remote-access.md)
