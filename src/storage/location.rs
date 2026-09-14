//! Safe parsing and display of local paths and explicit object-store locations.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use url::Url;

use crate::error::{NcvError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Local,
    S3,
    Gcs,
    Azure,
}

impl Provider {
    pub const fn is_remote(self) -> bool {
        !matches!(self, Self::Local)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceLocation {
    provider: Provider,
    container: Option<String>,
    azure_account: Option<String>,
    object_key: String,
    local_path: Option<PathBuf>,
    display: String,
}

impl fmt::Debug for SourceLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceLocation")
            .field("provider", &self.provider)
            .field("container", &self.container)
            .field("azure_account", &self.azure_account)
            .field("object_key", &self.object_key)
            .field("local_path", &self.local_path)
            .field("display", &self.display)
            .finish()
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display)
    }
}

impl SourceLocation {
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.is_empty() {
            return Err(invalid("location is empty"));
        }
        if raw.chars().any(char::is_control) {
            return Err(invalid("location contains a control character"));
        }

        if !raw.contains("://") {
            return Ok(Self {
                provider: Provider::Local,
                container: None,
                azure_account: None,
                object_key: String::new(),
                local_path: Some(PathBuf::from(raw)),
                display: raw.to_owned(),
            });
        }

        let raw_authority_and_path = raw
            .split_once("://")
            .map(|(_, remainder)| remainder)
            .ok_or_else(|| invalid("malformed object-store URI"))?;
        if let Some(raw_path) = raw_authority_and_path.split_once('/').map(|(_, path)| path) {
            let raw_path = raw_path.split(['?', '#']).next().unwrap_or_default();
            validate_key_segments(raw_path)?;
        }

        let url = Url::parse(raw).map_err(|_| invalid("malformed object-store URI"))?;
        if !url.username().is_empty() && !matches!(url.scheme(), "abfs" | "abfss") {
            return Err(invalid("credentials are not accepted in a source location"));
        }
        if url.password().is_some() {
            return Err(invalid("credentials are not accepted in a source location"));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(invalid(
                "query and fragment material are not accepted in a source location",
            ));
        }

        let scheme = url.scheme().to_ascii_lowercase();
        let provider = match scheme.as_str() {
            "s3" => Provider::S3,
            "gs" => Provider::Gcs,
            "az" | "abfs" | "abfss" => Provider::Azure,
            _ => return Err(invalid("unsupported object-store scheme")),
        };

        let container = if matches!(provider, Provider::Azure) && !url.username().is_empty() {
            url.username().to_owned()
        } else {
            url.host_str().unwrap_or_default().to_owned()
        };
        validate_component(&container, "container")?;
        let azure_account = if matches!(provider, Provider::Azure) && !url.username().is_empty() {
            url.host_str()
                .and_then(|host| host.split('.').next())
                .filter(|account| !account.is_empty())
                .map(str::to_owned)
        } else {
            None
        };

        let object_key = url.path().trim_start_matches('/').to_owned();
        validate_key(&object_key)?;

        let display = if matches!(provider, Provider::Azure) && !url.username().is_empty() {
            format!(
                "{}://{}@{}/{}",
                scheme,
                container,
                url.host_str().unwrap_or_default(),
                object_key
            )
        } else {
            format!("{}://{}/{}", scheme, container, object_key)
        };
        Ok(Self {
            provider,
            container: Some(container),
            azure_account,
            object_key,
            local_path: None,
            display,
        })
    }

    pub const fn provider(&self) -> Provider {
        self.provider
    }

    pub fn container(&self) -> Option<&str> {
        self.container.as_deref()
    }

    pub fn azure_account(&self) -> Option<&str> {
        self.azure_account.as_deref()
    }

    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    pub fn local_path(&self) -> Option<&Path> {
        self.local_path.as_deref()
    }

    pub const fn is_remote(&self) -> bool {
        self.provider.is_remote()
    }

    pub fn safe_display(&self) -> &str {
        &self.display
    }

    /// Return a sibling object location while retaining the same provider and container.
    ///
    /// This is used for optional colocated metadata such as a GRIB2 `.idx` object. The suffix
    /// is deliberately restricted to a simple object-key suffix and is never interpreted as a
    /// URL query or credential material.
    pub fn with_object_suffix(&self, suffix: &str) -> Result<Self> {
        if !self.is_remote() || suffix.is_empty() || suffix.contains(['/', '?', '#']) {
            return Err(invalid("invalid sibling object suffix"));
        }
        let key = format!("{}{}", self.object_key, suffix);
        let display = format!("{}{}", self.display, suffix);
        let mut sibling = self.clone();
        sibling.object_key = key;
        sibling.display = display;
        validate_key(&sibling.object_key)?;
        Ok(sibling)
    }
}

fn invalid(reason: &str) -> NcvError {
    NcvError::InvalidSourceLocation {
        reason: reason.to_owned(),
    }
}

fn validate_component(component: &str, name: &str) -> Result<()> {
    if component.is_empty() {
        return Err(invalid(&format!("{name} is missing")));
    }
    if component == "." || component == ".." || component.contains('/') {
        return Err(invalid(&format!("{name} is malformed")));
    }
    if component.chars().any(|character| character.is_control()) {
        return Err(invalid(&format!("{name} contains a control character")));
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(invalid("object key is missing"));
    }
    validate_key_segments(key)?;
    if key
        .chars()
        .any(|character| matches!(character, '*' | '?' | '[' | ']'))
    {
        return Err(invalid("remote object globs are not supported"));
    }
    Ok(())
}

fn validate_key_segments(key: &str) -> Result<()> {
    if key.split('/').any(|segment| {
        segment == "."
            || segment == ".."
            || segment.eq_ignore_ascii_case("%2e")
            || segment.eq_ignore_ascii_case("%2e%2e")
    }) {
        return Err(invalid("object key contains a traversal segment"));
    }
    Ok(())
}
