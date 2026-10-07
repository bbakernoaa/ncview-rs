---
type: cheatsheet
category: reference
tags: [keyboard, mouse, shortcuts, cheatsheet, controls]
---

# Keyboard & mouse reference

Every interactive control, in one table. This mirrors the in-app help (`?`).

## Keyboard

| Key | Action |
| --- | --- |
| `q` / `Esc` | quit (`Esc` closes a dialog first) |
| `↑` / `↓` | previous / next variable |
| `←` / `→` | previous / next time slice |
| `<` / `>` | previous / next time slice |
| `Space` | play / pause the timeline |
| `-` / `+` | decrease / increase playback speed |
| `{` / `}` | previous / next file |
| `[` / `]` | previous / next depth slice |
| `,` / `.` | select previous / next extra dimension |
| `;` / `'` | previous / next index in selected dimension |
| `Tab` | focus the sidebar level list (`Tab`/`Esc` leave it); in dialogs/plot chooser it switches field/axis |
| `Shift`+arrows | pan the zoomed map |
| `c` | open colormap chooser with a focused preview |
| `v` | reverse the active colormap; in the chooser, reverse the focused preview |
| `i` | cycle interpolation (set `NCVIEW_SCIENTIFIC_RENDERING=0` to unlock) |
| `e` | export current slice (PNG + SVG + JSON) |
| `a` | automatic limits |
| `l` | edit min/max limits |
| `f` | mask data outside a range |
| `s` | toggle linear/log color scale |
| `z` | toggle current/global color scale |
| `r` | reset zoom |
| `g` | logical / projected grid |
| `b` | toggle filled land/ocean map backdrop |
| `Enter` | open pinned-point plot menu |
| `p` | open plot chooser |
| `t` / `d` / `h` | plot chooser: time series / scatter / histogram |
| `k` / `u` | plot chooser: CDF / vertical profile |
| `m` | add/remove the hovered point from the plot selection |
| `Ctrl-P / :` | command palette — search actions, `Enter` to run |
| `/` | browse and search all plottable variables |
| `?` | open/close this help |

### Limits dialog

Type numbers directly, `Tab` switches between min and max fields, `Enter`
applies, `Esc` cancels.

## Mouse

| Gesture | Action |
| --- | --- |
| Move over map | hover row/col/value readout in the status bar |
| Click map | pin a point (`◆`), then `Enter` or `p` for plot choices |
| Hover + `m` | accumulate multiple points for multi-trace plots |
| Click | sidebar buttons, variables, timeline play/pause, speed controls |
| Click / drag the level bar | seek the vertical level |
| Wheel over the sidebar | scroll the level or variable list |
| Drag map | zoom to a rectangle; drag a zoomed map to pan |
| `Shift`+drag | zoom again while already zoomed |
| Right-click or `?` | close this help |

## Related

- [Command-line reference](cli.md)
- [Environment reference](environment.md)
- [Style the map](../how-to/style-the-map.md)
