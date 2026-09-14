# Remote streaming fixtures

Remote tests use deterministic, credential-free fixtures served by an in-process fake object
store. The fixture owner is the repository maintainer; every committed object must have a
SHA-256 checksum, a redistribution-compatible license, and a short description of the scientific
semantics it exercises.

Do not commit provider credentials, signed URLs, authorization headers, private bucket names, or
operational data. Live S3, GCS, and Azure tests are opt-in and must skip when their provider
environment is absent.

Small fixtures may be committed when they are useful for unit and contract tests. Large or
generated fixtures must be reproducible from a checked-in generator and documented with their
generation command, expected size, checksum, and memory/transfer purpose; they must not be fetched
implicitly by the test suite. Tests should generate temporary large objects from those recipes or
use the fake store's deterministic byte source.

The current credential-free corpus is composed from the checked-in NetCDF-4 fixtures under
`tests/fixtures/` and the optional repository GRIB2 input used by `tests/remote_grib2.rs` when it
is present. The large-object NetCDF test pads a deterministic fixture in memory; it does not commit
operational data. Reproduce the remote checks with:

```bash
cargo test --locked --test remote_netcdf4
cargo test --locked --test remote_grib2
```
