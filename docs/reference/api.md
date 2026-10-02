---
type: reference
category: reference
tags: [api, rustdoc, library, modules, architecture]
---

# Library API reference

`ncview-rs` exposes its internals as a Rust library (`ncview-rs`) alongside the
`ncv` binary. The full, always-current API documentation is **generated from
the code's own rustdoc comments** and published with this site under the
[`api/`](../api/index.html) section.

- Crate: `ncview-rs` (MIT OR Apache-2.0)
- Minimum Rust: 1.90 (edition 2024)
- Public surface: `#![deny(unsafe_code)]` — the whole crate is safe Rust

## Module map

| Module | Responsibility |
| --- | --- |
| `data` | Read-only dataset and slice abstractions: NetCDF-4/HDF5, GRIB2 (local and remote), coordinates, diff, formula-derived variables (`data::formula::FormulaSource`), reference manifests, virtual datasets |
| `storage` | Provider-neutral remote object access: locations (`s3://`, `gs://`, `az://`, `abfs[s]://`), range cache, typed operations, session state. All network work lives behind this module and its background runtime |
| `render` | Color normalization and terminal raster rendering: palettes, land mask, map background, image-protocol selection, viewport-bounded rasterization |
| `ui` | Ratatui view composition: dashboard, canvas, colorbar, sidebar, level bar, timeline, charts, help, popups, layout |
| `events` | Terminal event and session handling: keyboard input, mouse, terminal enter/restore lifecycle |
| `analysis` | Scientific analysis and coordinate-mapping primitives: mapping, projection, time series, equation-editor formula parsing and evaluation (`analysis::formula`) |
| `app` | Application state: view model, generation counters, variable/palette/limits state, decoded working-set bounds |
| `export` | File exporters for the current scientific view (PNG, SVG, JSON sidecar) |
| `error` | `NcvError` — actionable errors carrying file/variable/dimension/capability context |

## Reading the generated docs

The `api/` section of this site is rebuilt automatically whenever the code
changes, so signatures, doc comments, and trait implementations always match
the source at that commit. It includes:

- every public type, function, and method with its rustdoc,
- the `NcvError` variants and their messages,
- module-level overviews (the table above comes from them).

For behavior contracts, the narrative pages are the entry point:

- [Data fidelity](../explanation/data-fidelity.md) — what values guarantee
- [Terminal protocols](../explanation/terminal-protocols.md) — rendering selection
- [Remote access](../explanation/remote-access.md) — byte-range I/O design
