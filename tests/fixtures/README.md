# Synthetic fixtures

Remote-streaming tests serve byte-identical copies of these deterministic fixtures through the
in-process object-store test double. These small fixtures are intentionally committed so a clean
checkout can compile and run the tests; the ignore rules for local datasets explicitly exempt them.
Large-object coverage pads a fixture at runtime, so no cloud object or generated operational dataset
is committed. Optional live-provider checks must remain environment-gated and must never record
credentials.

Fixture builders use small, redistributable arrays with documented expected values. The
COARDS/CF NetCDF-4 fixture is generated from `coards-float32.cdl`, so tests do not depend on a
machine-specific climate-data path.

`coards-float32.nc4` models the relevant MACCity conventions: float32 `Pixel_area(date, lat, lon)`
and `MACCity(date, lat, lon)` fields, one-dimensional `date`, `lat`, and `lon` coordinates, CF
metadata, chunking, and DEFLATE compression. Their known values are 101–160 and 1–60; the
regression test reads both variables and checks their 4×5 slices. Regenerate it with:

```sh
ncgen -4 -o tests/fixtures/coards-float32.nc4 tests/fixtures/coards-float32.cdl
```

Bounded-zoom coverage generates additional small fixtures in `tests/data_fidelity.rs`: signed
longitude coordinates (-180 through 180), 0–360 longitude coordinates, dimension-only axes with no
coordinate variables (which use zero-based cell indices), and the committed curvilinear grid. The
generated arrays use ascending latitude/longitude axes; unit tests also cover descending coordinate
vectors and invalid ranges directly.

## Raw-Dimension Display Fixture

Feature 007 builds deterministic synthetic NetCDF-4 fixtures in temporary directories rather than
requiring the local VIIRS product file. The raw 2D, VIIRS-like 3D, and 4D test inputs are defined in
`tests/raw_dimension_view.rs`. Values in the 3D case are `row * 100 + column * 10 + kernel`, so
the kernel planes are independently observable. These fixtures have no geographic coordinate
variables; height and width are the plane axes and the remaining dimension is selectable.

An optional integration regression reads the real `VIIRS_BRDF_LSA_NBAR_2025057_h19v19.nc` from
the repository root when supplied locally. It is not copied, modified, or required in CI. A packed
synthetic end-to-end fixture is not currently committed because the lightweight writer cannot
create it and the current reader does not expose the packed primary variable in the attempted
`ncgen` output; packed decoding remains covered separately by decoder fidelity tests.

Generated fixture hashes (SHA-256):

- `regular.nc4`: `9759f4bc44bc326c93dc0f8aeefda44f945a2673f207d7696f61c901d5fbcd48`
- `packed-fill.nc4`: `9759f4bc44bc326c93dc0f8aeefda44f945a2673f207d7696f61c901d5fbcd48`
- `curvilinear.nc4`: `abc16631330c6df1777b4b2d8b1e54e15ed211a25dbcdaffd3e1a5f178c1fbbb`
- `netcdf3-unsupported.nc`: `d71eff333e000bb7d3e1856b65e23650801ccc9c4851aa9da01eca756b49aa64`
- `corrupt.nc`: `8141db4372deed39e7986ae7cbe7faef02e34c55f73b886a01750abb5dd3442c`
- `coards-float32.nc4`: `40f0517505fd85e4e30300ee9dae9b59c71086f2ea4db1c7552c6ca87313f197`
