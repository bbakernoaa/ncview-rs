---
type: howto
category: how_to
tags: [grib2, kerchunk, virtualizarr, manifest, idx, sidecar, reference]
---

# Generate a GRIB2 reference manifest

Goal: turn a GRIB2 file and its NOAA-style `.idx` sidecar into a deterministic
Kerchunk-compatible reference JSON that tools like VirtualiZarr (or `ncv`
itself) can open as a virtual dataset.

## The command

```bash
ncv manifest \
  --format kerchunk \
  --input forecast.grib2 \
  --idx forecast.grib2.idx \
  --output forecast.refs.json \
  --strict
```

| Flag | Meaning |
| --- | --- |
| `--format` | `kerchunk` or `virtualizarr` (the VirtualiZarr Kerchunk-parser profile) |
| `--input` | GRIB2 source object |
| `--idx` | matching `.idx` sidecar |
| `--output` | destination manifest JSON |
| `--source-uri` | URI to embed in byte-range references instead of the local path (use this when the manifest will point at a remote object) |
| `--strict` | treat warnings and mismatches as errors |

## What the manifest preserves

The manifest keeps raw GRIB2 table codes and the full Section 4 product
payload, so fields that share a display short name (for example aerosol
`AOTK`) remain independently selectable by species, particle-size interval,
wavelength, and product-template context. Opening the manifest later shows each
species as its own variable.

## Use it

```bash
ncv forecast.refs.json        # open the virtual dataset directly — no download
ncv s3://bucket/forecast.refs.json   # or pair it with the remote source object
```

Reference manifests describe byte ranges — they never copy or modify the source
GRIB2 data.

## Table inventory

The pinned NOAA/NCEP GRIB2 table inventory and its update policy live in
`tools/grib2_tables/README.md` in the repository.

## Related

- [Access remote data](access-remote-data.md)
- [CLI reference](../reference/cli.md)
- [Supported formats](../reference/formats.md)
