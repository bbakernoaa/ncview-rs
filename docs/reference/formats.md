---
type: reference
category: reference
tags: [formats, netcdf, grib2, kerchunk, hdf5, compatibility, platforms]
---

# Supported formats and environments

## Inputs `ncv` opens

| Input | Forms | Notes |
| --- | --- | --- |
| NetCDF-4 (HDF5) | `.nc`, `.nc4`, `.hdf5`-magic NetCDF-4 | Local and remote; subgroups appear as qualified names (`physics/temperature`) |
| GRIB2 | `.grib`, `.grib2`, `.grb`, `.grb2`, GRIB-magic detection | Local and remote; remote objects use `.idx` sidecars or bounded range reads |
| Kerchunk / VirtualiZarr reference manifests | `.json` | Virtual datasets describing byte ranges; never copies or modifies source data |
| Remote object locations | `s3://`, `gs://`, `az://`, `abfs[s]://` | See [Access remote data](../how-to/access-remote-data.md) |

## Inputs `ncv` rejects (on purpose)

| Input | Behavior |
| --- | --- |
| NetCDF-3 | Rejected **before terminal entry** with an actionable diagnostic |
| Arbitrary HDF5 (non-NetCDF-4) | Rejected with an actionable diagnostic |
| Large unindexed remote GRIB2 | Refused rather than silently downloaded in full |

Rejection happens before the TUI starts, so your terminal is never left in a
modified state.

## Platforms

Release archives are published for:

| OS | Architecture |
| --- | --- |
| Linux | x86_64 (glibc) and x86_64 musl (static, for older distributions) |
| macOS | arm64 |
| Windows | x86_64 |

Each release ships a `SHA256SUMS` file. The binary is pure Rust with no native
NetCDF/HDF5 runtime dependency.

## Terminals and SSH

- Image protocols: Kitty, Sixel, iTerm2 (auto-detected after terminal entry;
  overridable with `NCVIEW_IMAGE_PROTOCOL`).
- Portable fallback: `ncv`'s own cell rasterizer — one truecolor background
  block per terminal cell — always functional, including over OpenSSH and tmux.
- Explicit `NCVIEW_IMAGE_PROTOCOL` settings are honored as given on all terminals; the portable cell renderer remains available with `NCVIEW_IMAGE_PROTOCOL=cells`.
- No X11 forwarding required.

## Data safety

All datasets are opened **read-only**. Source files are never modified.
Missing, fill, NaN, and infinity values are preserved and masked honestly —
see [Data fidelity](../explanation/data-fidelity.md).

## Related

- [CLI reference](cli.md)
- [Environment reference](environment.md)
