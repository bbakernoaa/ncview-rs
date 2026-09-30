---
type: howto
category: how_to
tags: [remote, s3, gcs, azure, anonymous, object-store, public-bucket]
---

# Access remote data

Goal: open NetCDF-4 or GRIB2 objects stored in S3, GCS, or Azure Blob without
downloading them first.

## Accepted location forms

```bash
ncv s3://bucket/key
ncv gs://bucket/key
ncv az://container/key
ncv abfs://container@account.dfs.core.windows.net/key   # and abfss://
```

The terminal starts immediately; remote metadata and range reads happen on a
loader thread, so the UI never blocks on the network.

## Credentials

Ambient provider configuration is used when present (`AWS_*`, `GOOGLE_*`,
`AZURE_*`). **Secrets are never accepted in arguments and never persisted.**

## Public buckets without credentials

On a laptop or any host with no cloud credentials, `ncv` detects that the
instance metadata service is unreachable and reads public buckets (for example
`s3://noaa-gefs-pds/...`) with **anonymous, credential-free access** instead
of stalling on metadata-token retries.

| Want | Do this |
| --- | --- |
| Force anonymous mode | `NCVIEW_ANONYMOUS_ACCESS=1` |
| Keep the automatic decision | Unset it, or set any non-truthy value such as `0` |

See [Remote access explanation](../explanation/remote-access.md) for why the
metadata probe matters.

## GRIB2 over the network

Remote GRIB2 objects use bounded `HEAD`/range requests, plus an optional
colocated `.idx` sidecar when available. A large **unindexed** GRIB2 object is
refused rather than silently downloaded in full — add the `.idx` sidecar (or
generate a reference manifest, see [Generate a GRIB2 manifest](generate-grib2-manifest.md)).

## Remote NetCDF-4

Objects up to 64 MiB use a bounded fallback byte reader. Larger objects use a
bounded metadata window and source-backed HDF5 range reads for chunk indexes,
compressed chunks, masks, coordinates, and selected rows — for these larger
objects, the whole object is never materialized. Unsupported HDF5 layouts fail
with an actionable diagnostic rather than a partial view.

## Related

- [Quickstart](../tutorials/quickstart.md)
- [Environment reference](../reference/environment.md)
- [Supported formats](../reference/formats.md)
