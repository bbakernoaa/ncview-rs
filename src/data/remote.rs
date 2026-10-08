//! Remote dataset dispatch and bounded format sniffing.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use bytes::Bytes;

use super::{
    DataSource, DatasetMetadata, PointCoordinates, remote_grib2, remote_netcdf4,
    slice::{Slice2D, SliceRequest},
};
use crate::{
    app::Generation,
    error::{NcvError, Result},
    storage::{
        StorageRuntime,
        location::SourceLocation,
        object_store::{ByteRange, RemoteStore, build_provider_store},
        operation::OperationState,
        receive,
    },
};

const MAGIC_BYTES: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessCapabilities {
    pub range_reads: bool,
    pub chunked_reads: bool,
    pub sidecar_index: bool,
    pub curvilinear_coordinates: bool,
    pub bounded_fallback: bool,
}

/// Metadata-first handle for one remote source.
///
/// Opening publishes this session after metadata/index work completes. The first slice is still
/// scheduled independently by the application, so metadata can be exposed without making the
/// terminal event loop wait for a rendered frame.
pub struct RemoteDatasetSession {
    reference: SourceLocation,
    identity: crate::storage::object_store::ObjectIdentity,
    metadata: DatasetMetadata,
    capabilities: AccessCapabilities,
    source: Arc<dyn DataSource>,
    pub operation_state: OperationState,
}

impl RemoteDatasetSession {
    fn new(
        source: Box<dyn DataSource>,
        identity: crate::storage::object_store::ObjectIdentity,
        capabilities: AccessCapabilities,
    ) -> Self {
        let source: Arc<dyn DataSource> = Arc::from(source);
        Self {
            reference: identity.location().clone(),
            identity,
            metadata: source.metadata().clone(),
            capabilities,
            source,
            operation_state: OperationState::default(),
        }
    }

    pub fn reference(&self) -> &SourceLocation {
        &self.reference
    }

    pub fn identity(&self) -> &crate::storage::object_store::ObjectIdentity {
        &self.identity
    }

    pub const fn capabilities(&self) -> AccessCapabilities {
        self.capabilities
    }

    pub fn source(&self) -> &dyn DataSource {
        self.source.as_ref()
    }

    pub fn begin_operation(
        &self,
        generation: Generation,
        kind: crate::storage::operation::OperationKind,
    ) -> crate::storage::operation::RemoteOperation {
        self.operation_state
            .begin(generation, kind, self.reference.clone())
    }
}

/// Open a cloud object without materializing it as a local file.
pub fn open_remote(source: SourceLocation) -> Result<Box<dyn DataSource>> {
    open_remote_with_progress(source, &|_| true)
}

pub fn open_remote_with_progress(
    source: SourceLocation,
    progress: &dyn Fn(&str) -> bool,
) -> Result<Box<dyn DataSource>> {
    let store = build_provider_store(&source)?;
    open_remote_with_store_progress(source, store, progress)
}

/// Testable/provider-injected form of [`open_remote`]. Production callers normally use the
/// ambient credential-chain builder above; tests and embedding applications can supply an
/// object-store implementation without changing the source-location contract.
pub fn open_remote_with_store(
    source: SourceLocation,
    store: Arc<dyn object_store::ObjectStore>,
) -> Result<Box<dyn DataSource>> {
    open_remote_with_store_progress(source, store, &|_| true)
}

pub fn open_remote_with_store_progress(
    source: SourceLocation,
    store: Arc<dyn object_store::ObjectStore>,
    progress: &dyn Fn(&str) -> bool,
) -> Result<Box<dyn DataSource>> {
    report(progress, "connecting to remote object")?;
    let runtime = Arc::new(StorageRuntime::spawn()?);
    let remote = Arc::new(RemoteStore::new(source.clone(), store));
    let identity = receive(&runtime, {
        let remote = Arc::clone(&remote);
        async move { remote.head().await }
    })??;
    report(
        progress,
        &format!("remote object: {} bytes", identity.size()),
    )?;
    let magic = if identity.size() == 0 {
        Bytes::new()
    } else {
        let end = identity.size().min(MAGIC_BYTES);
        let range = ByteRange::new(0, end, identity.size())?;
        receive(&runtime, {
            let remote = Arc::clone(&remote);
            let identity = identity.clone();
            async move { remote.read_range(&identity, range).await }
        })??
    };
    report(progress, "remote format identified")?;

    let (source, capabilities) = if is_grib2(&source, &magic) {
        (
            remote_grib2::open_remote_with_progress(remote, identity.clone(), runtime, progress)?,
            AccessCapabilities {
                range_reads: true,
                chunked_reads: false,
                sidecar_index: true,
                curvilinear_coordinates: false,
                bounded_fallback: true,
            },
        )
    } else if magic.starts_with(b"\x89HDF\r\n\x1a\n") {
        (
            remote_netcdf4::open_remote_with_progress(remote, identity.clone(), runtime, progress)?,
            AccessCapabilities {
                range_reads: true,
                chunked_reads: identity.size() > 64 * 1024 * 1024,
                sidecar_index: false,
                curvilinear_coordinates: true,
                bounded_fallback: identity.size() <= 64 * 1024 * 1024,
            },
        )
    } else if is_manifest(&source, &magic) {
        let full_range = ByteRange::new(0, identity.size(), identity.size())?;
        let bytes = receive(&runtime, {
            let remote = Arc::clone(&remote);
            let identity = identity.clone();
            async move { remote.read_range(&identity, full_range).await }
        })??;
        let json_text = String::from_utf8_lossy(&bytes);
        let manifest_source = super::manifest::ManifestSource::from_json_str(
            &json_text,
            source.safe_display(),
            std::path::PathBuf::from("."),
            true,
        )?;
        (
            Box::new(manifest_source) as Box<dyn DataSource>,
            AccessCapabilities {
                range_reads: true,
                chunked_reads: false,
                sidecar_index: false,
                curvilinear_coordinates: false,
                bounded_fallback: true,
            },
        )
    } else {
        Err(NcvError::remote_failure(
            source,
            "format detection",
            "object is neither a supported GRIB2 object, NetCDF-4/HDF5 object, nor VirtualiZarr/Icechunk manifest",
        ))?
    };
    Ok(Box::new(RemoteDatasetSession::new(
        source,
        identity,
        capabilities,
    )))
}

fn report(progress: &dyn Fn(&str) -> bool, message: &str) -> Result<()> {
    if progress(message) {
        Ok(())
    } else {
        Err(NcvError::WorkerStopped)
    }
}

fn is_manifest(source: &SourceLocation, magic: &[u8]) -> bool {
    source
        .object_key()
        .rsplit_once('.')
        .is_some_and(|(_, extension)| {
            matches!(extension.to_ascii_lowercase().as_str(), "json" | "manifest")
        })
        || magic.starts_with(b"{")
        || magic.starts_with(b"[")
}

fn is_grib2(source: &SourceLocation, magic: &[u8]) -> bool {
    magic.starts_with(b"GRIB")
        || source
            .object_key()
            .rsplit_once('.')
            .is_some_and(|(_, extension)| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "grib" | "grib2" | "grb" | "grb2"
                )
            })
}

impl DataSource for RemoteDatasetSession {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn is_remote(&self) -> bool {
        true
    }

    fn source_identity(&self) -> Option<&str> {
        Some(self.identity.cache_token())
    }

    fn remote_capabilities(&self) -> Option<AccessCapabilities> {
        Some(self.capabilities)
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.source.read_slice(request)
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        self.source
            .read_slice_on_axes(request, row_axis, col_axis, fixed_axes)
    }

    fn read_slice_on_axes_cancellable(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
        cancelled: Arc<AtomicBool>,
    ) -> Result<Slice2D> {
        self.source
            .read_slice_on_axes_cancellable(request, row_axis, col_axis, fixed_axes, cancelled)
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.source.time_label(index)
    }

    fn time_label_for_variable(&self, variable: &str, index: usize) -> Option<String> {
        self.source.time_label_for_variable(variable, index)
    }

    fn vertical_label(&self, variable: &str, index: usize) -> Option<String> {
        self.source.vertical_label(variable, index)
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        self.source.dimension_values(variable, dimension)
    }

    fn point_coordinates(&self, variable: &str, row: usize, col: usize) -> PointCoordinates {
        self.source.point_coordinates(variable, row, col)
    }
}
