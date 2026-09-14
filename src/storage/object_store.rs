//! Object-store provider construction and validated range operations.

use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use bytes::Bytes;
use object_store::{GetOptions, ObjectMeta, ObjectStore, ObjectStoreExt, path::Path as ObjectPath};

use crate::{
    error::{NcvError, Result},
    storage::{
        location::SourceLocation,
        operation::{is_retryable_message, retry_delay},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub(crate) start: u64,
    pub(crate) end: u64,
}

impl ByteRange {
    pub fn new(start: u64, end: u64, object_size: u64) -> Result<Self> {
        if start > end {
            return Err(NcvError::InvalidRange("start is after end".to_owned()));
        }
        if end > object_size {
            return Err(NcvError::InvalidRange(format!(
                "range end {end} exceeds object size {object_size}"
            )));
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn end(self) -> u64 {
        self.end
    }

    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn as_range(self) -> Range<u64> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectIdentity {
    location: SourceLocation,
    size: u64,
    etag: Option<String>,
    version: Option<String>,
    last_modified: String,
    cache_token: String,
    verified: bool,
}

impl ObjectIdentity {
    fn from_meta(location: SourceLocation, meta: &ObjectMeta) -> Self {
        let version_token = meta.version.as_deref().or(meta.e_tag.as_deref());
        let suffix = version_token.map_or_else(
            || format!("size:{}:modified:{}", meta.size, meta.last_modified),
            ToOwned::to_owned,
        );
        let cache_token = format!("{}#{suffix}", location.safe_display());
        Self {
            location,
            size: meta.size,
            etag: meta.e_tag.clone(),
            version: meta.version.clone(),
            last_modified: meta.last_modified.to_rfc3339(),
            cache_token,
            verified: meta.e_tag.is_some() || meta.version.is_some(),
        }
    }

    pub fn location(&self) -> &SourceLocation {
        &self.location
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn last_modified(&self) -> &str {
        &self.last_modified
    }

    pub fn cache_token(&self) -> &str {
        &self.cache_token
    }

    pub const fn verified(&self) -> bool {
        self.verified
    }

    fn matches_meta(&self, meta: &ObjectMeta) -> bool {
        meta.size == self.size
            && self
                .etag
                .as_ref()
                .is_none_or(|etag| meta.e_tag.as_ref() == Some(etag))
            && self
                .version
                .as_ref()
                .is_none_or(|version| meta.version.as_ref() == Some(version))
    }
}

#[derive(Debug, Clone)]
pub struct RangeBlock {
    identity: String,
    range: ByteRange,
    bytes: Bytes,
    fetched_at: Instant,
}

impl RangeBlock {
    pub fn new(identity: &ObjectIdentity, range: ByteRange, bytes: Bytes) -> Result<Self> {
        if bytes.len() as u64 != range.len() {
            return Err(NcvError::InvalidRange(format!(
                "received {} bytes for requested {}-byte range",
                bytes.len(),
                range.len()
            )));
        }
        Ok(Self {
            identity: identity.cache_token.clone(),
            range,
            bytes,
            fetched_at: Instant::now(),
        })
    }

    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    pub const fn range(&self) -> ByteRange {
        self.range
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub const fn fetched_at(&self) -> Instant {
        self.fetched_at
    }
}

pub struct RemoteStore {
    source: SourceLocation,
    object_path: ObjectPath,
    store: Arc<dyn ObjectStore>,
    policy: RangePolicy,
    request_log: Arc<Mutex<Vec<ByteRange>>>,
    stats: Arc<Mutex<RemoteStats>>,
}

#[derive(Debug, Clone, Copy)]
pub struct RangePolicy {
    pub max_request_bytes: u64,
    pub max_attempts: u32,
    pub request_timeout: Duration,
    pub retry_base: Duration,
    pub retry_maximum: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteStats {
    pub requests: u64,
    pub requested_bytes: u64,
    pub received_bytes: u64,
    pub retries: u64,
    pub elapsed_millis: u64,
}

/// Safe, serializable transfer diagnostics for status panes and logs.
///
/// This intentionally contains the redacted source label and aggregate
/// counters only; provider clients and credential material never enter the
/// diagnostic object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDiagnostics {
    pub provider: String,
    pub source: String,
    pub operation: String,
    pub object_size: u64,
    pub requests: u64,
    pub requested_bytes: u64,
    pub received_bytes: u64,
    pub retries: u64,
    pub elapsed_millis: u64,
}

impl Default for RangePolicy {
    fn default() -> Self {
        Self {
            max_request_bytes: 64 * 1024 * 1024,
            max_attempts: 3,
            request_timeout: Duration::from_secs(30),
            retry_base: Duration::from_millis(100),
            retry_maximum: Duration::from_secs(2),
        }
    }
}

#[derive(Clone, Default)]
pub struct ProviderStoreRegistry {
    stores: Arc<RwLock<HashMap<String, Arc<dyn ObjectStore>>>>,
}

impl ProviderStoreRegistry {
    pub fn register(&self, provider_key: impl Into<String>, store: Arc<dyn ObjectStore>) {
        if let Ok(mut stores) = self.stores.write() {
            stores.insert(provider_key.into(), store);
        }
    }

    pub fn get(&self, provider_key: &str) -> Option<Arc<dyn ObjectStore>> {
        self.stores.read().ok()?.get(provider_key).cloned()
    }
}

pub fn build_provider_store(source: &SourceLocation) -> Result<Arc<dyn ObjectStore>> {
    if !source.is_remote() {
        return Err(NcvError::remote_failure(
            source.clone(),
            "provider",
            "a local path does not have an object-store provider",
        ));
    }
    let container = source.container().ok_or_else(|| {
        NcvError::remote_failure(source.clone(), "provider", "remote container is missing")
    })?;
    install_crypto_provider();
    let store: Arc<dyn ObjectStore> = match source.provider() {
        crate::storage::location::Provider::S3 => Arc::new(
            object_store::aws::AmazonS3Builder::from_env()
                .with_bucket_name(container)
                .build()
                .map_err(|error| {
                    NcvError::remote_failure(source.clone(), "provider", &error.to_string())
                })?,
        ),
        crate::storage::location::Provider::Gcs => Arc::new(
            object_store::gcp::GoogleCloudStorageBuilder::from_env()
                .with_bucket_name(container)
                .build()
                .map_err(|error| {
                    NcvError::remote_failure(source.clone(), "provider", &error.to_string())
                })?,
        ),
        crate::storage::location::Provider::Azure => Arc::new(
            source
                .azure_account()
                .map_or_else(
                    object_store::azure::MicrosoftAzureBuilder::from_env,
                    |account| {
                        object_store::azure::MicrosoftAzureBuilder::from_env().with_account(account)
                    },
                )
                .with_container_name(container)
                .build()
                .map_err(|error| {
                    NcvError::remote_failure(source.clone(), "provider", &error.to_string())
                })?,
        ),
        crate::storage::location::Provider::Local => unreachable!(),
    };
    Ok(store)
}

pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl RemoteStore {
    pub fn new(source: SourceLocation, store: Arc<dyn ObjectStore>) -> Self {
        Self::with_policy(source, store, RangePolicy::default())
    }

    pub fn with_policy(
        source: SourceLocation,
        store: Arc<dyn ObjectStore>,
        policy: RangePolicy,
    ) -> Self {
        let object_path = ObjectPath::from(source.object_key());
        Self {
            source,
            object_path,
            store,
            policy,
            request_log: Arc::new(Mutex::new(Vec::new())),
            stats: Arc::new(Mutex::new(RemoteStats::default())),
        }
    }

    pub fn source(&self) -> &SourceLocation {
        &self.source
    }

    pub const fn max_request_bytes(&self) -> u64 {
        self.policy.max_request_bytes
    }

    pub fn with_source(&self, source: SourceLocation) -> Self {
        let mut remote = Self::with_policy(source, Arc::clone(&self.store), self.policy);
        remote.request_log = Arc::clone(&self.request_log);
        remote.stats = Arc::clone(&self.stats);
        remote
    }

    pub fn requested_ranges(&self) -> Vec<ByteRange> {
        self.request_log
            .lock()
            .map_or_else(|_| Vec::new(), |log| log.clone())
    }

    pub fn stats(&self) -> RemoteStats {
        self.stats
            .lock()
            .map_or_else(|_| RemoteStats::default(), |stats| *stats)
    }

    pub fn diagnostics(
        &self,
        identity: &ObjectIdentity,
        operation: impl Into<String>,
    ) -> RemoteDiagnostics {
        let stats = self.stats();
        RemoteDiagnostics {
            provider: format!("{:?}", self.source.provider()),
            source: self.source.safe_display().to_owned(),
            operation: operation.into(),
            object_size: identity.size(),
            requests: stats.requests,
            requested_bytes: stats.requested_bytes,
            received_bytes: stats.received_bytes,
            retries: stats.retries,
            elapsed_millis: stats.elapsed_millis,
        }
    }

    pub async fn head(&self) -> Result<ObjectIdentity> {
        let attempts = self.policy.max_attempts.max(1);
        for attempt in 0..attempts {
            let result = tokio::time::timeout(
                self.policy.request_timeout,
                self.store.head(&self.object_path),
            )
            .await;
            match result {
                Ok(Ok(meta)) => return Ok(ObjectIdentity::from_meta(self.source.clone(), &meta)),
                Ok(Err(error)) => {
                    let message = error.to_string();
                    if attempt + 1 < attempts && is_retryable_message(&message) {
                        tokio::time::sleep(retry_delay(
                            attempt,
                            self.policy.retry_base,
                            self.policy.retry_maximum,
                        ))
                        .await;
                        continue;
                    }
                    return Err(NcvError::remote_failure(
                        self.source.clone(),
                        "HEAD",
                        &message,
                    ));
                }
                Err(_) => {
                    if attempt + 1 < attempts {
                        tokio::time::sleep(retry_delay(
                            attempt,
                            self.policy.retry_base,
                            self.policy.retry_maximum,
                        ))
                        .await;
                        continue;
                    }
                    return Err(NcvError::remote_failure(
                        self.source.clone(),
                        "HEAD",
                        "request timed out",
                    ));
                }
            }
        }
        Err(NcvError::remote_failure(
            self.source.clone(),
            "HEAD",
            "request exhausted its retry budget",
        ))
    }

    pub async fn read_range(&self, identity: &ObjectIdentity, range: ByteRange) -> Result<Bytes> {
        let started = Instant::now();
        if identity.location != self.source {
            return Err(NcvError::remote_failure(
                self.source.clone(),
                "range",
                "object identity belongs to a different source",
            ));
        }
        ByteRange::new(range.start, range.end, identity.size)?;
        if range.len() > self.policy.max_request_bytes {
            return Err(NcvError::remote_failure(
                self.source.clone(),
                "range",
                "requested range exceeds the configured request-size limit",
            ));
        }
        if let Ok(mut log) = self.request_log.lock() {
            log.push(range);
        }
        if let Ok(mut stats) = self.stats.lock() {
            stats.requests = stats.requests.saturating_add(1);
            stats.requested_bytes = stats.requested_bytes.saturating_add(range.len());
        }
        let attempts = self.policy.max_attempts.max(1);
        for attempt in 0..attempts {
            let mut options = GetOptions::new().with_range(Some(range.as_range()));
            if let Some(etag) = identity.etag.clone() {
                options = options.with_if_match(Some(etag));
            }
            if let Some(version) = identity.version.clone() {
                options = options.with_version(Some(version));
            }
            let result = match tokio::time::timeout(
                self.policy.request_timeout,
                self.store.get_opts(&self.object_path, options),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => {
                    let message = error.to_string();
                    if attempt + 1 < attempts && is_retryable_message(&message) {
                        if let Ok(mut stats) = self.stats.lock() {
                            stats.retries = stats.retries.saturating_add(1);
                        }
                        tokio::time::sleep(retry_delay(
                            attempt,
                            self.policy.retry_base,
                            self.policy.retry_maximum,
                        ))
                        .await;
                        continue;
                    }
                    return Err(NcvError::remote_failure(
                        self.source.clone(),
                        "range",
                        &message,
                    ));
                }
                Err(_) => {
                    if attempt + 1 < attempts {
                        if let Ok(mut stats) = self.stats.lock() {
                            stats.retries = stats.retries.saturating_add(1);
                        }
                        tokio::time::sleep(retry_delay(
                            attempt,
                            self.policy.retry_base,
                            self.policy.retry_maximum,
                        ))
                        .await;
                        continue;
                    }
                    return Err(NcvError::remote_failure(
                        self.source.clone(),
                        "range",
                        "request timed out",
                    ));
                }
            };
            if !identity.matches_meta(&result.meta) || result.range != range.as_range() {
                return Err(NcvError::remote_failure(
                    self.source.clone(),
                    "range",
                    "provider returned bytes for a different object identity or interval",
                ));
            }
            let bytes =
                match tokio::time::timeout(self.policy.request_timeout, result.bytes()).await {
                    Ok(Ok(bytes)) => bytes,
                    Ok(Err(error)) => {
                        return Err(NcvError::remote_failure(
                            self.source.clone(),
                            "range",
                            &error.to_string(),
                        ));
                    }
                    Err(_) => {
                        return Err(NcvError::remote_failure(
                            self.source.clone(),
                            "range",
                            "request timed out while receiving bytes",
                        ));
                    }
                };
            RangeBlock::new(identity, range, bytes.clone())?;
            if let Ok(mut stats) = self.stats.lock() {
                stats.received_bytes = stats.received_bytes.saturating_add(bytes.len() as u64);
                stats.elapsed_millis = stats
                    .elapsed_millis
                    .saturating_add(started.elapsed().as_millis() as u64);
            }
            return Ok(bytes);
        }
        Err(NcvError::remote_failure(
            self.source.clone(),
            "range",
            "request exhausted its retry budget",
        ))
    }
}
