---
type: reference
category: reference
tags: [environment, configuration, variables, settings]
---

# Environment reference

Runtime configuration is done with environment variables. Set them before
launching `ncv`.

| Variable | Values | Default | Purpose |
| --- | --- | --- | --- |
| `NCVIEW_THREADS` | integer | `min(CPUs, 8)` | Parallel rasterization threads (`RAYON_NUM_THREADS` is also honored). The conservative default avoids hogging HPC head-node resources. |
| `NCVIEW_IMAGE_PROTOCOL` | `kitty`, `sixel`, `iterm2`, `cells` | auto-detected | Override graphics capability detection. Auto-detection identifies iTerm2 from environment hints; Kitty and Sixel are selected only through this override or a positive capability match. An explicit setting is honored as given. |
| `NCVIEW_SCIENTIFIC_RENDERING` | `1` / `0` | `1` (locked) | Keep nearest-neighbor scientific rendering, or allow interpolation (`0`). |
| `NCVIEW_IMAGE_FILTER` | `nearest`, `lanczos3`, `catmull-rom`, `triangle`, `gaussian` | `nearest` | Image filter when scientific rendering is unlocked. |
| `NCVIEW_CELL_PIXEL_SIZE` | `WxH` pixels, e.g. `10x20` | from the terminal | Terminal cell size used to size graphics images and map mouse clicks. Set it when the terminal does not report usable pixel dimensions (common over SSH); known 640x480 placeholders and reports under 4x8 pixels per cell are ignored, and `10x20` is assumed. |
| `NCVIEW_ANONYMOUS_ACCESS` | truthy (`1`, `true`, `yes`, `on`, `y`) | auto | Force credential-free access for public S3/GCS/Azure objects. Any other value, including `0`, leaves the automatic decision in charge rather than forcing signed access. Auto mode uses ambient credentials if present, otherwise a cached metadata-endpoint probe, and selects anonymous access when neither yields credentials. |
| `NCVIEW_LAND_DETAIL` | `110m`, `50m`, `10m`, `auto` | `110m` | Coastline resolution after enabling the `b` backdrop; `auto` selects by zoom level. |
| `NCVIEW_UG_INTERMEDIATE_SPACING_DEG` | degrees, `0 < x <= 90` | `0.25` | Cell size of the regular latitude/longitude grid that unstructured-grid fields (currently MPAS) are regridded onto. Smaller values sharpen the map but cost more per read. |
| `NCVIEW_COLORMAPS` | directory | — | Extra directory of `.ncmap` files for the palette catalog. |
| `NCVIEW_LIB_DIR` | directory | — | Also searched for colormaps, including `share/ncview/colormaps`. |
| `NCVIEWBASE` | directory | — | Also searched for colormaps, including `share/ncview/colormaps`. |
| `NCVIEW_EXPORT_DIR` | directory | current directory | Where `e` writes exports. |
| `NCVIEW_EXPORT_BACKGROUND` | `transparent`, `white` | `transparent` | Export canvas background. |
| `NCVIEW_EXPORT_TEXT` | `dark`, `light` | dark | Export text/border color (`light` for dark slides). |
| `NCVIEW_EXPORT_FONT` | font list | `Fira Code, monospace` | SVG export font selection (PNG always uses the embedded font). |
| `NCVIEW_SESSION_DIR` | directory | platform default | Where per-dataset session state is stored. |

## Notes

- Secrets are never read from arguments; cloud credentials come from ambient
  provider configuration (`AWS_*`, `GOOGLE_*`, `AZURE_*`).
- `NCVIEW_IMAGE_FILTER` and `i`-cycling only take effect when
  `NCVIEW_SCIENTIFIC_RENDERING=0`; the default preserves exact scientific
  cells.
- Malformed `.ncmap` files discovered via any colormap path are ignored at
  startup rather than causing an error.

## Related

- [Run over SSH](../how-to/run-over-ssh.md)
- [Access remote data](../how-to/access-remote-data.md)
- [Terminal protocols explanation](../explanation/terminal-protocols.md)
