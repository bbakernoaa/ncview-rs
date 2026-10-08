---
type: howto
category: how_to
tags: [timeline, playback, depth, level-bar, files, session, zoom]
---

# Navigate time, depth, and files

Goal: move through the fourth dimension efficiently — animate time, seek
vertical levels, and switch between open datasets.

## Step and play the time axis

| Task | Do this |
| --- | --- |
| Previous / next time slice | `←` / `→` or `<` / `>` |
| Play / pause the timeline | `Space`, or click the Play/Pause button |
| Playback speed | `-` / `+` or the labeled speed controls on the timeline |
| Seek to a date | click or drag the timeline gauge |
| Switch active file | `{` / `}` (the header shows "opened file N/M") |

When several files form one collection, the timeline spans all of them and
file switching keeps you on the corresponding time slot of the new file.

## Seek vertical levels

For variables with a depth/level axis, a full-width **Level bar** sits above
the time bar showing `index/N` plus the CF level label (falling back to the
dimension name when the file has none):

- **Click or drag** the level bar to seek.
- **Scroll** over it to step one level.
- `[` / `]` step levels from anywhere.
- `Tab` focuses the sidebar **Level list**; `↑`/`↓` move the cursor, `Enter`
  applies it, `Tab`/`Esc` leave focus.

Two-dimensional variables hide the level bar entirely — no dead controls.

## Zoom to a region

Drag across the map to zoom to a rectangle. To enter numeric bounds, open the
command palette with `Ctrl-P` or `:`, search for **Set view bounds**, and enter
minimum and maximum x/y values. `Tab` moves between fields; `Enter` applies;
`Esc` cancels. The existing box zoom and pan controls remain available.

For geographic axes, longitude accepts signed degrees or `0..360`, and
latitude must be within `-90..90`. The viewer maps coordinate endpoints to the
smallest source-index range containing the samples. Other axes use their
one-dimensional coordinate values when available, or inclusive zero-based
indices when they are dimension-only. Invalid or non-overlapping bounds leave
the accepted view unchanged. Curvilinear grids use a row/column envelope and
disclose that approximation in the status line.

Selecting a different variable returns to that variable's global view, since
its coordinate system and domain may differ. The zoom for a dataset is still
remembered between launches; explicit CLI bounds override the restored zoom.
See the [CLI reference](../reference/cli.md) for startup examples.

## Session restore

`ncv` remembers your last view (variable, slice, limits) per dataset and
restores it on the next launch. To start clean:

```bash
ncv --no-restore data.nc
```

The session directory can be relocated with `NCVIEW_SESSION_DIR` (useful on
shared HPC head nodes). See [Environment reference](../reference/environment.md).

## Related

- [Inspect a variable](inspect-a-variable.md)
- [Keyboard reference](../reference/keyboard.md)
