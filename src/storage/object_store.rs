//! Object-store provider construction and validated range operations.

use std::{
    collections::HashMap,
    env,
    net::{IpAddr, SocketAddr, TcpStream},
    ops::Range,
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::{Duration, Instant},
};

use bytes::Bytes;
use object_store::{GetOptions, ObjectMeta, ObjectStore, ObjectStoreExt, path::Path as ObjectPath};

use crate::{
    error::{NcvError, Result},
    storage::{
        location::{Provider, SourceLocation},
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

/// Environment variable that forces credential-free (anonymous) access for
/// public object stores, skipping the provider credential chain entirely.
/// A truthy value forces anonymous access; any other value leaves the
/// automatic decision (ambient credentials, then metadata-endpoint probe) in
/// charge.
pub const ANONYMOUS_ACCESS_ENV: &str = "NCVIEW_ANONYMOUS_ACCESS";

/// Link-local address hosting the AWS, GCP, and Azure instance metadata
/// services. Unreachable on laptops and most networks, where a credential
/// chain that falls through to the metadata endpoint stalls on retries for
/// every object request.
const METADATA_LINK_LOCAL_IP: &str = "169.254.169.254";

/// Budget for the single metadata-endpoint reachability probe. An unroutable
/// link-local address fails immediately; a real metadata service answers well
/// inside this window.
const METADATA_PROBE_TIMEOUT: Duration = Duration::from_millis(800);

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

/// Decide whether to use anonymous (unsigned, credential-free) access for a
/// provider. The decision is pure so it can be tested without touching the
/// process environment or the network.
fn anonymous_access_decision(
    forced: bool,
    ambient_credentials: bool,
    metadata_reachable: bool,
) -> bool {
    forced || !(ambient_credentials || metadata_reachable)
}

/// Environment keys that place a provider's credential chain on a named
/// source other than the instance metadata endpoint, mirroring the ambient
/// configuration the pinned object_store builders read.
fn ambient_credential_env_vars(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::S3 => &[
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "AWS_ROLE_ARN",
            "AWS_WEB_IDENTITY_TOKEN_FILE",
            "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "AWS_SHARED_CREDENTIALS_FILE",
        ],
        Provider::Gcs => &[
            "GOOGLE_SERVICE_ACCOUNT",
            "SERVICE_ACCOUNT",
            "GOOGLE_SERVICE_ACCOUNT_KEY",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GOOGLE_BEARER_TOKEN",
        ],
        Provider::Azure => &[
            "AZURE_STORAGE_ACCOUNT_KEY",
            "AZURE_STORAGE_TOKEN",
            "AZURE_STORAGE_CLIENT_ID",
            "AZURE_STORAGE_CLIENT_SECRET",
            "AZURE_STORAGE_TENANT_ID",
            "AZURE_STORAGE_SAS_KEY",
            "AZURE_USE_AZURE_CLI",
            "IDENTITY_ENDPOINT",
        ],
        Provider::Local => &[],
    }
}

fn ambient_credentials_configured_with(
    lookup: &dyn Fn(&str) -> Option<String>,
    provider: Provider,
) -> bool {
    ambient_credential_env_vars(provider)
        .iter()
        .any(|name| lookup(name).is_some())
}

fn ambient_credentials_configured(provider: Provider) -> bool {
    ambient_credentials_configured_with(&env_value, provider)
}

fn explicit_anonymous_requested_with(lookup: &dyn Fn(&str) -> Option<String>) -> bool {
    lookup(ANONYMOUS_ACCESS_ENV).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "y"
        )
    })
}

fn explicit_anonymous_requested() -> bool {
    explicit_anonymous_requested_with(&env_value)
}

/// Parse a configured metadata endpoint (bare host, host:port, or absolute
/// URL) into the address to probe.
fn parse_endpoint_host_port(raw: &str) -> Option<(String, u16)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let candidate = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    };
    let url = url::Url::parse(&candidate).ok()?;
    let host = url.host_str()?.to_owned();
    Some((host, url.port_or_known_default().unwrap_or(80)))
}

fn configured_metadata_endpoints_with(
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, u16)> {
    let mut endpoints = vec![(METADATA_LINK_LOCAL_IP.to_owned(), 80u16)];
    for name in [
        "AWS_EC2_METADATA_SERVICE_ENDPOINT",
        "GCE_METADATA_HOST",
        "GCE_METADATA_IP",
        "MSI_ENDPOINT",
        "IDENTITY_ENDPOINT",
    ] {
        if let Some(endpoint) = lookup(name).as_deref().and_then(parse_endpoint_host_port) {
            endpoints.push(endpoint);
        }
    }
    endpoints
}

/// Only literal addresses are probed: a custom endpoint behind a hostname is
/// assumed to be a real metadata service, so the signed chain is kept.
fn tcp_host_reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(ip) = host.parse::<IpAddr>() else {
        return true;
    };
    TcpStream::connect_timeout(&SocketAddr::new(ip, port), timeout).is_ok()
}

/// Cached reachability of the cloud instance metadata services. Opening a
/// collection of sibling objects must pay the probe at most once per process.
fn metadata_endpoint_reachable() -> bool {
    static REACHABLE: OnceLock<bool> = OnceLock::new();
    *REACHABLE.get_or_init(|| {
        configured_metadata_endpoints_with(&env_value)
            .iter()
            .any(|(host, port)| tcp_host_reachable(host, *port, METADATA_PROBE_TIMEOUT))
    })
}

/// Whether this provider should build an unsigned, credential-free store.
/// Forced by [`ANONYMOUS_ACCESS_ENV`]; otherwise signed access is kept when
/// ambient credentials are configured or the instance metadata service is
/// reachable (an on-VM workload identity), and public anonymous access is used
/// when neither applies.
fn use_anonymous_access(provider: Provider) -> bool {
    let forced = explicit_anonymous_requested();
    let ambient = ambient_credentials_configured(provider);
    let reachable = if forced || ambient {
        false
    } else {
        metadata_endpoint_reachable()
    };
    anonymous_access_decision(forced, ambient, reachable)
}

/// Bound provider-internal retries so a missing or misconfigured credential
/// chain surfaces in seconds instead of stalling for minutes per object.
fn provider_retry_config() -> object_store::RetryConfig {
    object_store::RetryConfig {
        max_retries: 3,
        retry_timeout: Duration::from_secs(10),
        ..Default::default()
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
    let anonymous = use_anonymous_access(source.provider());
    let store: Arc<dyn ObjectStore> = match source.provider() {
        crate::storage::location::Provider::S3 => {
            let mut builder = object_store::aws::AmazonS3Builder::from_env()
                .with_bucket_name(container)
                .with_retry(provider_retry_config());
            if anonymous {
                builder = builder.with_skip_signature(true);
            }
            Arc::new(builder.build().map_err(|error| {
                NcvError::remote_failure(source.clone(), "provider", &error.to_string())
            })?)
        }
        crate::storage::location::Provider::Gcs => {
            let mut builder = object_store::gcp::GoogleCloudStorageBuilder::from_env()
                .with_bucket_name(container)
                .with_retry(provider_retry_config());
            if anonymous {
                builder = builder.with_skip_signature(true);
            }
            Arc::new(builder.build().map_err(|error| {
                NcvError::remote_failure(source.clone(), "provider", &error.to_string())
            })?)
        }
        crate::storage::location::Provider::Azure => {
            let mut builder = source
                .azure_account()
                .map_or_else(
                    object_store::azure::MicrosoftAzureBuilder::from_env,
                    |account| {
                        object_store::azure::MicrosoftAzureBuilder::from_env().with_account(account)
                    },
                )
                .with_container_name(container)
                .with_retry(provider_retry_config());
            if anonymous {
                builder = builder.with_skip_signature(true);
            }
            Arc::new(builder.build().map_err(|error| {
                NcvError::remote_failure(source.clone(), "provider", &error.to_string())
            })?)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map_lookup<'a>(map: &'a HashMap<String, String>) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            map.get(name)
                .cloned()
                .filter(|value| !value.trim().is_empty())
        }
    }

    #[test]
    fn anonymous_only_when_no_credentials_and_metadata_unreachable() {
        assert!(anonymous_access_decision(false, false, false));
        assert!(!anonymous_access_decision(false, true, false));
        assert!(!anonymous_access_decision(false, false, true));
        assert!(!anonymous_access_decision(false, true, true));
    }

    #[test]
    fn forced_anonymous_overrides_credentials_and_metadata() {
        assert!(anonymous_access_decision(true, false, false));
        assert!(anonymous_access_decision(true, true, false));
        assert!(anonymous_access_decision(true, false, true));
        assert!(anonymous_access_decision(true, true, true));
    }

    #[test]
    fn ambient_credentials_cover_each_provider_chain() {
        let map = HashMap::from([
            ("AWS_ACCESS_KEY_ID".to_owned(), "AKIA-example".to_owned()),
            (
                "GOOGLE_APPLICATION_CREDENTIALS".to_owned(),
                "/tmp/adc.json".to_owned(),
            ),
            (
                "AZURE_STORAGE_SAS_KEY".to_owned(),
                "sv=2024-01-01".to_owned(),
            ),
        ]);
        let lookup = map_lookup(&map);
        assert!(ambient_credentials_configured_with(&lookup, Provider::S3));
        assert!(ambient_credentials_configured_with(&lookup, Provider::Gcs));
        assert!(ambient_credentials_configured_with(
            &lookup,
            Provider::Azure
        ));
        assert!(!ambient_credentials_configured_with(
            &lookup,
            Provider::Local
        ));
    }

    #[test]
    fn empty_ambient_credential_values_are_not_configured() {
        let map = HashMap::from([
            ("AWS_ACCESS_KEY_ID".to_owned(), "   ".to_owned()),
            ("AWS_ROLE_ARN".to_owned(), String::new()),
        ]);
        let lookup = map_lookup(&map);
        assert!(!ambient_credentials_configured_with(&lookup, Provider::S3));
    }

    #[test]
    fn anonymous_env_flag_only_accepts_truthy_values() {
        for truthy in ["1", "true", "TRUE", " yes ", "on", "y"] {
            let map = HashMap::from([(ANONYMOUS_ACCESS_ENV.to_owned(), truthy.to_owned())]);
            let lookup = map_lookup(&map);
            assert!(
                explicit_anonymous_requested_with(&lookup),
                "{truthy:?} must force anonymous access"
            );
        }
        for falsy in ["0", "false", "no", "off", "auto", "", "   "] {
            let map = HashMap::from([(ANONYMOUS_ACCESS_ENV.to_owned(), falsy.to_owned())]);
            let lookup = map_lookup(&map);
            assert!(
                !explicit_anonymous_requested_with(&lookup),
                "{falsy:?} must keep automatic detection"
            );
        }
        assert!(!explicit_anonymous_requested_with(&|_| None));
    }

    #[test]
    fn metadata_endpoints_default_to_link_local_and_honor_config() {
        let default = configured_metadata_endpoints_with(&|_| None);
        assert_eq!(default, vec![("169.254.169.254".to_owned(), 80u16)]);

        let map = HashMap::from([
            (
                "AWS_EC2_METADATA_SERVICE_ENDPOINT".to_owned(),
                "http://169.254.170.2:1338".to_owned(),
            ),
            (
                "GCE_METADATA_HOST".to_owned(),
                "metadata.google.internal".to_owned(),
            ),
        ]);
        let lookup = map_lookup(&map);
        let endpoints = configured_metadata_endpoints_with(&lookup);
        assert_eq!(endpoints[0], ("169.254.169.254".to_owned(), 80u16));
        assert!(endpoints.contains(&("169.254.170.2".to_owned(), 1338u16)));
        assert!(endpoints.contains(&("metadata.google.internal".to_owned(), 80u16)));
    }

    #[test]
    fn endpoint_parsing_accepts_host_port_and_url() {
        assert_eq!(
            parse_endpoint_host_port("169.254.169.254"),
            Some(("169.254.169.254".to_owned(), 80))
        );
        assert_eq!(
            parse_endpoint_host_port("http://127.0.0.1:1338/"),
            Some(("127.0.0.1".to_owned(), 1338))
        );
        assert_eq!(
            parse_endpoint_host_port("127.0.0.1:1338"),
            Some(("127.0.0.1".to_owned(), 1338))
        );
        assert_eq!(parse_endpoint_host_port("  "), None);
    }

    #[test]
    fn probe_skips_named_hosts_and_fails_fast_on_unroutable_address() {
        // A hostname is assumed to be a deliberate metadata configuration.
        assert!(tcp_host_reachable(
            "metadata.google.internal",
            80,
            Duration::from_millis(50)
        ));
        // The unroutable link-local address must not hang the probe budget.
        let started = Instant::now();
        let reachable = tcp_host_reachable(METADATA_LINK_LOCAL_IP, 80, Duration::from_millis(50));
        assert!(!reachable || started.elapsed() < METADATA_PROBE_TIMEOUT * 4);
    }

    #[test]
    fn provider_retries_are_bounded() {
        let retry = provider_retry_config();
        assert_eq!(retry.max_retries, 3);
        assert_eq!(retry.retry_timeout, Duration::from_secs(10));
    }
}
