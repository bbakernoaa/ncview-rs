---
type: explanation
category: explanation
tags: [terminal, kitty, sixel, iterm2, cells, probing, ssh, tmux, fallback]
---

# How `ncv` chooses a terminal renderer

The map is a true-color image, but terminals speak different graphics
languages. `ncv` selects the best one automatically — and guarantees a usable
view when none exists.

## Capability detection, carefully

After entering the terminal session, the viewer settles on one of four renderers
— Kitty, Sixel, iTerm2, or the cell fallback. Capability detection is
deliberately conservative: querying capabilities over SSH or through
multiplexers can hang for seconds while waiting for escape-sequence responses,
so `ncv` avoids the blocking query path. Instead it derives the choice from the
observed window size and environment hints. Those hints can only identify
iTerm2 automatically; Kitty and Sixel are used when you request them explicitly
with `NCVIEW_IMAGE_PROTOCOL`, and the user can override anything it gets wrong.

The header always reports the active renderer (`Kitty truecolor`, `Sixel
truecolor`, `iTerm2 truecolor`, `cell fallback`, or `cells (graphics
unavailable)` after a protocol encoding failure), so you never have to guess
what is actually being used.

## The fallback is a first-class mode

If no image protocol is available — plain OpenSSH, tmux with passthrough
disabled, an older terminal — `ncv` renders the map with its own cell
rasterizer: one truecolor background block per terminal cell. Everything still
works: hover readouts, clicking, zoom, export. The project rule is **capability
enhancement must never become capability lockout**: the fanciest protocol is
never required for any workflow.

## Known protocol quirks

- **WezTerm does not implement the Kitty protocol.** Kitty's placeholder
  encoding prints as literal glyph garbage there without returning an error,
  so `ncv` recognizes WezTerm and downgrades a Kitty request (even an explicit
  `NCVIEW_IMAGE_PROTOCOL=kitty`) to Sixel. Sixel and iTerm2 work normally in
  WezTerm.
- **iTerm2 uses its own protocol** rather than Kitty/Sixel.
- **tmux and other multiplexers** may swallow capability queries; set
  `NCVIEW_IMAGE_PROTOCOL` explicitly to bypass capability detection for the
  protocol choice.

## Scaling choices

By default the image is nearest-neighbor upsampled so a zoomed scientific cell
remains one crisp cell — the pixels you see correspond to source values.
Interpolation filters (Lanczos, Catmull-Rom, triangle, Gaussian) are available
only after unlocking scientific rendering (`NCVIEW_SCIENTIFIC_RENDERING=0`),
making it explicit when a display is smoothing data rather than showing it.

## Related

- [Run over SSH](../how-to/run-over-ssh.md)
- [Environment reference](../reference/environment.md)
- [Data fidelity](data-fidelity.md)
