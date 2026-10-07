---
type: tutorial
category: tutorials
tags: [quickstart, installation, netcdf, grib2, beginner]
---

# Quickstart: your first view in five minutes

This tutorial takes you from an empty terminal to an interactive view of a
scientific variable, including a remote public-bucket dataset. By the end you
will know how to install `ncv`, open a file, confirm you are looking at real
values, and find your way around the dashboard.

**What you need**: a terminal on Linux (x86_64), macOS (arm64), or Windows
(x86_64), and any NetCDF-4 (HDF5) or GRIB2 file — or the public example below.

## 1. Install the viewer

### Option A — download a release (recommended)

Grab the platform archive from the
[GitHub Releases](https://github.com/bbakernoaa/ncview-rs/releases) page,
unpack it, and put `ncv` on your `PATH`:

```bash
tar -xzf ncv-linux-x86_64.tar.gz
install -m 755 ncv/ncv ~/.local/bin/ncv
ncv --version
# ncv {{NCV_VERSION}}
```

On older HPC distributions with an aging glibc, use the static build
`ncv-linux-x86_64-musl.tar.gz` instead. Every release ships a `SHA256SUMS`
file — verify before installing on shared systems.

### Option B — build from source

With Rust 1.90 or newer:

```bash
git clone https://github.com/bbakernoaa/ncview-rs.git
cd ncview-rs
cargo install --path . --locked
ncv --version
```

There is no native NetCDF/HDF5 runtime dependency to install.

## 2. Open a dataset

Point `ncv` at one or more files (shell globs are expanded for you):

```bash
ncv path/to/data.nc
ncv forecast*.grib2
```

You should land on the dashboard: a variable list on the left, the rendered
field in the middle, a colorbar on the right, and a timeline/level band along
the bottom. The status line shows the displayed slice's min/max/mean and a
finite count — your first fidelity check that real source values are in play.

## 3. Do the three things you will do most

1. **Change variable** — `↑`/`↓` moves through the variable list.
2. **Step through time** — `<`/`>` moves to the previous/next time slice;
   `Space` plays the timeline (`-`/`+` set playback speed).
3. **Get help** — `?` opens the keyboard/mouse reference; `Ctrl-P` (or `:`)
   opens a searchable command palette. Press `q` to quit — the terminal is
   always restored exactly as it was.

To start with a numeric region, give all four x/y bounds on the command line:

```bash
ncv --min-x -130 --max-x -60 --min-y 20 --max-y 55 path/to/data.nc
```

Inside the viewer, use `Ctrl-P` and choose **Set view bounds** to enter the
same four values. Both paths work with geographic coordinates and with
zero-based x/y indices when the dataset has no coordinate values. Box zoom
continues to work, and changing variables returns to that variable's global
view. See the [CLI reference](../reference/cli.md) for longitude conventions
and validation details.

## 4. Try a remote public bucket (optional)

`ncv` reads explicit object locations directly, with credential-free access
for public buckets:

```bash
ncv s3://noaa-gefs-pds/chem/2026/09/30/12/GEFS.chem.t12z.a2d_0p25.f000.grib2
```

If your environment has no cloud credentials, public buckets are read
anonymously without stalling. Force the mode with `NCVIEW_ANONYMOUS_ACCESS=1`;
any other value, including `=0`, leaves the choice to the automatic decision.
See
[Access remote data](../how-to/access-remote-data.md) for the details.

## 5. Know what you are looking at

- Values are never modified: missing, fill, NaN, and infinity cells are masked
  by the loader and reported honestly in the statistics.
- If the map looks like colored blocks rather than an image, you are on the
  portable **cell fallback** — one truecolor block per terminal cell, just
  less pretty than an image protocol. See
  [Run over SSH](../how-to/run-over-ssh.md).
- `e` exports the current slice as PNG + SVG + JSON for slides and notebooks.

## Next steps

- [Inspect a variable in detail](../how-to/inspect-a-variable.md)
- [Style the map: palettes, limits, masks](../how-to/style-the-map.md)
- [Full keyboard reference](../reference/keyboard.md)
- [Command-line reference](../reference/cli.md)
- [Why the values can be trusted](../explanation/data-fidelity.md)
