# GRIB2 table baseline

This directory records the complete document-entry universe used by the GRIB2
resolver. The pinned NOAA/NCEP baseline is the NCEP WMO GRIB2 documentation
version 37.0.0, published 2026-06-01. `inventory.json` includes the
introduction, revision history, Sections 0–8, Identification Templates 1.0–1.2,
appendices A–C, and every numbered table listed by the NOAA index—including
code, flag, template, and packing tables rather than only parameter tables.

Authoritative inputs:

- NOAA/NCEP: <https://www.nco.ncep.noaa.gov/pmb/docs/grib2/grib2_doc/>
- WMO Information System / GRIB2 table releases:
  <https://community.wmo.int/en/wis/latest-version>

The application retains raw table numbers whenever a snapshot does not define
an entry. This is required for centre-local values and for forward-compatible
WMO additions. A table update must:

1. record the source release and retrieval date in `inventory.json`;
2. update the table-ID list and generated lookup data together;
3. run `cargo test --locked --test grib2_catalog`; and
4. review aerosol/chemical entries for changes to species, size, wavelength,
   and optical qualifiers.

The inventory deliberately does not claim that a missing entry is a generic
meteorological variable: unresolved values are exposed as raw values with
table context and a deterministic diagnostic.
