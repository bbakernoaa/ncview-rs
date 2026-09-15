//! Remote NetCDF-4 dispatch boundary.
//!
//! Small objects retain the documented bounded complete-object fallback. Larger
//! objects use a bounded HDF5 metadata window plus source-backed chunk reads;
//! the complete object is never materialized.

use std::{path::Path, sync::Arc};

use oxih5::ByteSource;

use super::{DataSource, DatasetMetadata, PointCoordinates, netcdf4::NetCdf4Source};
use super::{
    remote_hdf5::{BlockingRemoteByteSource, RemoteByteSource},
    slice::{Slice2D, SliceRequest},
};
use crate::{
    error::{NcvError, Result},
    storage::{
        StorageRuntime,
        object_store::{ByteRange, ObjectIdentity, RemoteStore},
        receive,
    },
};

const MAX_BOUNDED_FALLBACK_BYTES: u64 = 64 * 1024 * 1024;
const INITIAL_METADATA_WINDOW_BYTES: u64 = 4 * 1024 * 1024;
const MAX_METADATA_WINDOW_BYTES: u64 = 64 * 1024 * 1024;
const HDF5_PAGE_SIZE: u64 = 1024 * 1024;
const HDF5_CACHE_BYTES: usize = 128 * 1024 * 1024;

/// Remote identity wrapper around the existing NetCDF-4 decoder.
///
/// The bounded fallback still decodes from memory, but marking the source as remote is
/// important: subsequent slice, coordinate, and time-series work must remain off the terminal
/// event loop even when the object itself fit within the fallback limit.
pub struct RemoteNetCdf4Source {
    inner: NetCdf4Source,
    identity: ObjectIdentity,
}

impl RemoteNetCdf4Source {
    pub fn identity(&self) -> &ObjectIdentity {
        &self.identity
    }
}

pub fn open_remote(
    remote: Arc<RemoteStore>,
    identity: ObjectIdentity,
    runtime: Arc<StorageRuntime>,
) -> Result<Box<dyn super::DataSource>> {
    open_remote_with_progress(remote, identity, runtime, &|_| true)
}

pub fn open_remote_with_progress(
    remote: Arc<RemoteStore>,
    identity: ObjectIdentity,
    runtime: Arc<StorageRuntime>,
    progress: &dyn Fn(&str) -> bool,
) -> Result<Box<dyn super::DataSource>> {
    report(progress, "preparing NetCDF-4 reader")?;
    let source_path = Path::new(remote.source().safe_display());
    let source = if identity.size() <= MAX_BOUNDED_FALLBACK_BYTES {
        report(progress, "fetching bounded NetCDF-4 object")?;
        let range = ByteRange::new(0, identity.size(), identity.size())?;
        let bytes = receive(&runtime, {
            let remote = Arc::clone(&remote);
            let identity = identity.clone();
            async move { remote.read_range(&identity, range).await }
        })??;
        report(progress, "decoding NetCDF-4 metadata")?;
        NetCdf4Source::open_bytes(source_path, &bytes).map_err(|error| {
            NcvError::remote_failure(
                remote.source().clone(),
                "NetCDF-4 decode",
                &error.to_string(),
            )
        })?
    } else {
        report(progress, "opening source-backed NetCDF-4 metadata")?;
        let paged = Arc::new(RemoteByteSource::new(
            Arc::clone(&remote),
            identity.clone(),
            HDF5_PAGE_SIZE,
            HDF5_CACHE_BYTES,
        ));
        let source_reader: Arc<dyn ByteSource> = Arc::new(BlockingRemoteByteSource::new(
            Arc::clone(&paged),
            Arc::clone(&runtime),
        ));
        let mut window = INITIAL_METADATA_WINDOW_BYTES.min(identity.size());
        loop {
            report(
                progress,
                &format!("reading NetCDF-4 metadata window ({window} bytes)"),
            )?;
            let metadata = source_reader
                .read(
                    0,
                    usize::try_from(window).map_err(|_| {
                        NcvError::remote_failure(
                            remote.source().clone(),
                            "NetCDF-4 metadata",
                            "metadata window exceeds addressable memory",
                        )
                    })?,
                )
                .map_err(|error| {
                    NcvError::remote_failure(
                        remote.source().clone(),
                        "NetCDF-4 metadata range",
                        &error.to_string(),
                    )
                })?;
            match NetCdf4Source::open_source(source_path, metadata, Arc::clone(&source_reader)) {
                Ok(source) => break source,
                Err(error) => {
                    if window >= MAX_METADATA_WINDOW_BYTES || window >= identity.size() {
                        let error = error.to_string();
                        return Err(NcvError::remote_failure(
                            remote.source().clone(),
                            "NetCDF-4 source-backed decode",
                            &error,
                        ));
                    }
                    window = window
                        .saturating_mul(2)
                        .min(MAX_METADATA_WINDOW_BYTES)
                        .min(identity.size());
                }
            }
        }
    };
    Ok(Box::new(RemoteNetCdf4Source {
        inner: source,
        identity,
    }))
}

impl DataSource for RemoteNetCdf4Source {
    fn metadata(&self) -> &DatasetMetadata {
        self.inner.metadata()
    }

    fn is_remote(&self) -> bool {
        true
    }

    fn source_identity(&self) -> Option<&str> {
        Some(self.identity.cache_token())
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.inner.read_slice(request)
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        self.inner
            .read_slice_on_axes(request, row_axis, col_axis, fixed_axes)
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.inner.time_label(index)
    }

    fn time_label_for_variable(&self, variable: &str, index: usize) -> Option<String> {
        self.inner.time_label_for_variable(variable, index)
    }

    fn vertical_label(&self, variable: &str, index: usize) -> Option<String> {
        self.inner.vertical_label(variable, index)
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        self.inner.dimension_values(variable, dimension)
    }

    fn point_coordinates(&self, variable: &str, row: usize, col: usize) -> PointCoordinates {
        self.inner.point_coordinates(variable, row, col)
    }
}

fn report(progress: &dyn Fn(&str) -> bool, message: &str) -> Result<()> {
    if progress(message) {
        Ok(())
    } else {
        Err(NcvError::WorkerStopped)
    }
}
