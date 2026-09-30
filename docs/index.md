---
type: project
category: reference
tags: [introduction, overview, ncv, documentation]
---

# `ncv` documentation

Documentation for `ncv` {{NCV_VERSION}}.

`ncv` is a terminal-native, read-only scientific data viewer for NetCDF-4,
GRIB2, and Kerchunk/VirtualiZarr reference manifests. It is designed for SSH
sessions on HPC systems and ships as a single Rust binary with no native
NetCDF/HDF5 runtime dependency.

The focus is **fast inspection, not editing**: slice a variable, inspect exact
values, compare time/depth frames, and export a publication-ready view — all
without leaving the terminal.

## How these docs are organized

This site follows the [Diátaxis](https://diataxis.fr/) framework — four
quadrants for four different reader needs:

| Quadrant | Answers | Start here |
| --- | --- | --- |
| [Tutorials](tutorials/index.md) | "Teach me the basics" | [Quickstart](tutorials/quickstart.md) |
| [How-To Guides](how-to/index.md) | "Help me do a specific task" | [Inspect a variable](how-to/inspect-a-variable.md) |
| [Reference](reference/index.md) | "Tell me exactly what X does" | [Keyboard](reference/keyboard.md) · [CLI](reference/cli.md) · [Environment](reference/environment.md) |
| [Explanation](explanation/index.md) | "Help me understand why" | [Data fidelity](explanation/data-fidelity.md) |

## At a glance

- **Open**: `ncv data.nc`, `ncv forecast*.grib2`, or `ncv s3://bucket/key`
- **Navigate**: `↑`/`↓` variables, `<`/`>` time, `[`/`]` depth, `{`/`}` files
- **Style**: `c` palette, `v` reverse, `a`/`l` limits, `f` mask, `s` scale, `z` scope
- **Analyze**: `Space` playback, click to pin, `Enter`/`p` for plots
- **Share**: `e` exports PNG + SVG + JSON
- **Help**: `?` in-app help, `Ctrl-P`/`:` command palette, `q` quits cleanly

## Get the tool

Download from [GitHub Releases](https://github.com/bbakernoaa/ncview-rs/releases)
or build with `cargo install --path . --locked`. See the
[Quickstart](tutorials/quickstart.md) for both paths.

Source, issues, and releases live at
<https://github.com/bbakernoaa/ncview-rs>.
