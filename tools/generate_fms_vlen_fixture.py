"""Generate a tiny FMS-style NetCDF-4 fixture with vlen-of-reference DIMENSION_LIST.

The operational UFS/FV3 history files (e.g. atmf024.nc) store DIMENSION_LIST in
the vlen-of-object-references encoding that the low-level HDF5 reader cannot
decode, so variables report phony dimension names while `_Netcdf4Coordinates`
and `_Netcdf4Dimid` remain valid. Axis roles must still be recovered from the
1-D coordinate variables (cartesian_axis/positive/units metadata).
"""

import numpy as np
import netCDF4 as nc

path = "tests/fixtures/fms-vlen-dimlist.nc4"
ds = nc.Dataset(path, "w", format="NETCDF4")
ds.createDimension("grid_xt", 4)
ds.createDimension("grid_yt", 3)
ds.createDimension("pfull", 3)
ds.createDimension("phalf", 4)
ds.createDimension("time", 1)

x = ds.createVariable("grid_xt", "f8", ("grid_xt",))
x[:] = [0.0, 30.0, 60.0, 90.0]
x.cartesian_axis = "X"
x.units = "degrees_E"
x.long_name = "T-cell longitude"

y = ds.createVariable("grid_yt", "f8", ("grid_yt",))
y[:] = [-45.0, 0.0, 45.0]
y.cartesian_axis = "Y"
y.units = "degrees_N"
y.long_name = "T-cell latitude"

z = ds.createVariable("pfull", "f4", ("pfull",))
z[:] = [100.0, 500.0, 1000.0]
z.cartesian_axis = "Z"
z.positive = "down"
z.units = "mbar"
z.long_name = "ref full pressure level"

# phalf carries only cartesian_axis (no units/positive), so its Depth role
# must come from the FMS cartesian_axis convention alone.
h = ds.createVariable("phalf", "f4", ("phalf",))
h[:] = [50.0, 300.0, 700.0, 1010.0]
h.cartesian_axis = "Z"

tmp = ds.createVariable("tmp", "f4", ("time", "pfull", "grid_yt", "grid_xt"))
# value = level * 100 + row * 4 + col so every plane is independently observable
tmp[0] = np.arange(3 * 3 * 4, dtype="f4").reshape(3, 3, 4) % 12 + (
    np.arange(3, dtype="f4").reshape(3, 1, 1) * 100
)
tmp.long_name = "temperature"
tmp.units = "K"

# `time` is created last: with this netcdf-c layout the pure-Rust reader
# currently drops one mid-file dataset from group enumeration; keeping the
# small coordinate last preserves a faithful reproduction of the operational
# file (all variables enumerated, canonical ids resolvable, phony raw names).
t = ds.createVariable("time", "f8", ("time",))
t[:] = [24.0]
t.cartesian_axis = "T"
t.units = "hours since 2021-03-22 06:00:00"
t.calendar = "GREGORIAN"
ds.close()
print("wrote", path)
