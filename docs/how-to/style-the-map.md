---
type: howto
category: how_to
tags: [colormap, palette, limits, mask, scaling, landmask, zoom]
---

# Style the map

Goal: get a publication-worthy view — choose a colormap, set data limits,
mask values, pick a scale mode, and add geographic context.

## Choose and tune a colormap

| Task | Do this |
| --- | --- |
| Cycle built-in palettes | `c` or the sidebar palette button |
| Reverse the palette | `v` |
| Add custom `.ncmap` files | put them in the working directory, or set `NCVIEW_COLORMAPS=/path/to/maps` (`NCVIEW_LIB_DIR` and `NCVIEWBASE` are also searched, including `share/ncview/colormaps`) |

Built-ins: Viridis, Plasma, Turbo, Inferno, Magma, Cividis, Cool, Warm,
Cubehelix, Spectral. Malformed `.ncmap` files are ignored at startup, so a bad
file can never break the viewer.

## Set data limits

| Task | Do this |
| --- | --- |
| Automatic limits from the displayed slice | `a` |
| Type exact min/max | `l` — type into a field, `Tab` switches fields, `Enter` applies, `Esc` cancels |
| Mask everything outside a range | `f` |
| Linear ↔ logarithmic color scale | `s` |
| Scale scope: current-slice vs unzoomed-global | `z` or the sidebar scale-scope button |
| Reset zoom | `r` |

Manual limits always take precedence over either automatic mode. `_FillValue`
and missing values stay masked automatically regardless of your settings.

## Zoom and pan

- **Drag** across the map to zoom to a rectangle.
- Drag a zoomed map to **pan**, or use `Shift`+arrows.
- `Shift`-drag while already zoomed zooms again inside the current view.
- `r` returns to the full map.

Zooming changes which values the *current-slice* automatic limits see; switch
the scope to `global` with `z` if you want colors to stay stable while zooming.

## Geographic backdrop

Press `b` to layer a D3-inspired ocean, graticule, and filled Natural Earth
land backdrop beneath the raster. Valid data composites on top; fill/missing
cells reveal the geography. It is a **presentation aid only** — it never
changes data values or replaces a dataset-provided land/sea mask.

Select coastline detail with `NCVIEW_LAND_DETAIL=auto|110m|50m|10m`
(110m is the fast default once enabled; the overlay starts disabled so first
render stays light).

## Export the result

Press `e` to write the current variable/time/depth slice as PNG + SVG + JSON.
See [Compare and export](compare-and-export.md).

## Related

- [Environment reference](../reference/environment.md)
- [Data fidelity explanation](../explanation/data-fidelity.md)
