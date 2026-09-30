---
type: howto
category: how_to
tags: [ssh, terminal, kitty, sixel, iterm2, cells, fallback, tmux]
---

# Run over SSH

Goal: get the best map rendering on a remote HPC or cloud session, and know
the portable fallback when graphics protocols are unavailable.

## It just works (usually)

Connect with SSH and run `ncv` as usual. `ncv` detects the terminal's image
capability after entering the session and uses Kitty, Sixel, or iTerm2 graphics
when available. The header always reports the active renderer:

```
Kitty truecolor · Sixel truecolor · iTerm2 truecolor · cell fallback
```

No X11 forwarding, native NetCDF/HDF5 runtime, or Chafa installation is needed.

## The cell fallback is always available

If your terminal (or a multiplexer) supports no image protocol, `ncv` renders
the map with its own cell rasterizer: one truecolor background block per
terminal cell. It is fully functional — hover, click, zoom, and export all
work. Capability enhancement never becomes capability lockout.

## Force a protocol when probing is blocked

tmux and some multiplexers swallow the capability query. Tell `ncv` what to
use:

```bash
NCVIEW_IMAGE_PROTOCOL=kitty  ncv data.nc
NCVIEW_IMAGE_PROTOCOL=sixel  ncv data.nc
NCVIEW_IMAGE_PROTOCOL=iterm2 ncv data.nc
NCVIEW_IMAGE_PROTOCOL=cells  ncv data.nc   # conservative, works everywhere
```

A conservative SSH launch that always renders:

```bash
NCVIEW_IMAGE_PROTOCOL=cells ncv data.nc
```

## WezTerm note

WezTerm does **not** implement the Kitty graphics protocol. A Kitty request
there is automatically downgraded to Sixel so the map renders instead of
printing escape-sequence garbage. Sixel and iTerm2 both work in WezTerm.

## Keep scientific cells crisp

By default the map is nearest-neighbor upsampled so zoomed scientific cells
stay sharp. To allow interpolation:

```bash
NCVIEW_SCIENTIFIC_RENDERING=0 NCVIEW_IMAGE_FILTER=lanczos3 ncv data.nc
```

(Then `i` cycles filters interactively. See
[Terminal protocols explanation](../explanation/terminal-protocols.md).)

## Related

- [Environment reference](../reference/environment.md)
- [Quickstart](../tutorials/quickstart.md)
