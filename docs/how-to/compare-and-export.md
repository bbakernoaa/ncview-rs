---
type: howto
category: how_to
tags: [diff, export, png, svg, json, slides, publication]
---

# Compare and export

Goal: see the difference between two datasets, and export a slice for a
presentation or notebook.

## Difference mode

Compare two files — or two sets of files — side by side:

```bash
ncv --diff control.nc experiment.nc
ncv --diff first="run1*.nc" second="run2*.nc"
```

actionable error otherwise.
For multiple files on each side, use the explicit `first=` (or `1=`) and
`second=` (or `2=`) prefixes to keep the two sets clear. Without prefixes, the
first file is side one and all remaining files are side two. Diff mode needs at
least one file on each side and reports an actionable error otherwise.

## Export the current view

Press `e` to write the current variable/time/depth slice as three artifacts:

| File | Use it for |
| --- | --- |
| **PNG** | direct insertion into slides/reports — map, colorbar, and tick marks on a 16:9 canvas |
| **SVG** | editable text and full metadata; scales to any size |
| **JSON** sidecar | machine-readable CF/COARDS names, units, limits, and slice coordinates |

### Tune the export

| Want | Set |
| --- | --- |
| Output directory | `NCVIEW_EXPORT_DIR=/path` |
| Opaque white canvas | `NCVIEW_EXPORT_BACKGROUND=white` (default is transparent) |
| Light text for dark slides | `NCVIEW_EXPORT_TEXT=light` |
| Custom SVG font | `NCVIEW_EXPORT_FONT="Iosevka Nerd Font, sans-serif"` |

PNG exports always use the embedded font so they stay deterministic on machines
without your chosen font installed.

## Related

- [Style the map](style-the-map.md)
- [Environment reference](../reference/environment.md)
