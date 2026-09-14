//! Range-backed HDF5 primitives used by the NetCDF-4 adapter.

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use bytes::Bytes;

use crate::{
    error::Result,
    storage::{
        object_store::{ByteRange, ObjectIdentity, RangeBlock, RemoteStore},
        range_cache::{CacheCategory, RangeCache},
    },
};

/// Synchronous façade required by OxiH5's parser API. The actual object-store
/// request remains on the shared Tokio storage runtime; this type only bridges
/// the existing synchronous data-source contract to that runtime.
pub struct BlockingRemoteByteSource {
    source: Arc<RemoteByteSource>,
    runtime: Arc<crate::storage::StorageRuntime>,
}

impl BlockingRemoteByteSource {
    pub fn new(
        source: Arc<RemoteByteSource>,
        runtime: Arc<crate::storage::StorageRuntime>,
    ) -> Self {
        Self { source, runtime }
    }
}

impl fmt::Debug for BlockingRemoteByteSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockingRemoteByteSource")
            .field("identity", self.source.identity())
            .field("page_size", &self.source.page_size())
            .finish_non_exhaustive()
    }
}

impl oxih5::ByteSource for BlockingRemoteByteSource {
    fn len(&self) -> u64 {
        self.source.identity().size()
    }

    fn read(&self, offset: u64, length: usize) -> std::result::Result<Vec<u8>, oxih5::OxiH5Error> {
        let source = Arc::clone(&self.source);
        let receiver = self
            .runtime
            .submit(async move { source.read(offset, length as u64).await })
            .map_err(|error| oxih5::OxiH5Error::Format(error.to_string()))?;
        let bytes = receiver
            .recv()
            .map_err(|_| oxih5::OxiH5Error::Format("storage runtime stopped".into()))?
            .map_err(|error| oxih5::OxiH5Error::Format(error.to_string()))?;
        Ok(bytes.to_vec())
    }
}

/// A bounded random-access byte source for remote HDF5 metadata and chunks.
///
/// It deliberately exposes range reads rather than an `open_from_bytes` fallback. The blocking
/// adapter below connects this page cache to OxiH5's source-backed parser.
pub struct RemoteByteSource {
    remote: Arc<RemoteStore>,
    identity: ObjectIdentity,
    page_size: u64,
    cache: Mutex<RangeCache>,
}

impl RemoteByteSource {
    pub fn new(
        remote: Arc<RemoteStore>,
        identity: ObjectIdentity,
        page_size: u64,
        cache_bytes: usize,
    ) -> Self {
        Self {
            remote,
            identity,
            page_size: page_size.max(1),
            cache: Mutex::new(RangeCache::new(cache_bytes)),
        }
    }

    pub fn identity(&self) -> &ObjectIdentity {
        &self.identity
    }

    pub async fn read(&self, offset: u64, length: u64) -> Result<Bytes> {
        if length == 0 {
            return Ok(Bytes::new());
        }
        let requested_end = offset
            .checked_add(length)
            .ok_or_else(|| crate::error::NcvError::InvalidRange("byte read overflows".into()))?;
        let requested = ByteRange::new(offset, requested_end, self.identity.size())?;
        let page_start = (offset / self.page_size) * self.page_size;
        let page_end = requested_end
            .div_ceil(self.page_size)
            .saturating_mul(self.page_size)
            .min(self.identity.size());
        let range = ByteRange::new(page_start, page_end, self.identity.size())?;
        if let Ok(mut cache) = self.cache.lock()
            && let Some(block) = cache.get(&self.identity, range)
        {
            let start = (requested.start() - range.start()) as usize;
            let end = start + requested.len() as usize;
            return Ok(block.bytes().slice(start..end));
        }
        let bytes = self.remote.read_range(&self.identity, range).await?;
        if let Ok(mut cache) = self.cache.lock() {
            let _ = cache.insert(
                CacheCategory::Metadata,
                RangeBlock::new(&self.identity, range, bytes.clone())?,
            );
        }
        let start = (requested.start() - range.start()) as usize;
        let end = start + requested.len() as usize;
        Ok(bytes.slice(start..end))
    }

    pub const fn page_size(&self) -> u64 {
        self.page_size
    }
}
