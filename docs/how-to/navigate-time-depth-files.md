---
type: howto
category: how_to
tags: [timeline, playback, depth, level-bar, files, session]
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
