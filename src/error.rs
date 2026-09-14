use std::path::PathBuf;

use thiserror::Error;

use crate::storage::location::SourceLocation;

#[derive(Debug, Error)]
pub enum NcvError {
    #[error("invalid source location: {reason}")]
    InvalidSourceLocation { reason: String },
    #[error("remote {operation} failed for {location}: {reason}")]
    RemoteOperation {
        location: Box<SourceLocation>,
        operation: String,
        reason: String,
    },
    #[error("unsupported dataset format for {path}: {reason}")]
    UnsupportedFormat { path: PathBuf, reason: String },
    #[error("invalid dataset {path}: {reason}")]
    InvalidDataset { path: PathBuf, reason: String },
    #[error("unsupported variable {variable}: {reason}")]
    UnsupportedVariable { variable: String, reason: String },
    #[error("invalid slice request: {0}")]
    InvalidSlice(String),
    #[error("invalid byte range: {0}")]
    InvalidRange(String),
    #[error("terminal capability unavailable: {0}")]
    TerminalCapability(String),
    #[error("worker stopped")]
    WorkerStopped,
    #[error("dataset adapter error for {path}: {reason}")]
    Adapter { path: PathBuf, reason: String },
    #[error("GRIB2 error for {path}: {reason}")]
    Grib2 { path: PathBuf, reason: String },
    #[error("I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl NcvError {
    pub fn remote_failure(source: SourceLocation, operation: &str, detail: &str) -> Self {
        Self::RemoteOperation {
            location: Box::new(source),
            operation: operation.to_owned(),
            reason: redact_sensitive(detail),
        }
    }
}

fn redact_sensitive(detail: &str) -> String {
    let mut redacted = detail.to_owned();

    if let Some(start) = find_case_insensitive(&redacted, "authorization:") {
        let end = redacted[start..]
            .find('\n')
            .map_or(redacted.len(), |offset| start + offset);
        redacted.replace_range(start..end, "Authorization: <redacted>");
    }

    for key in [
        "x-amz-signature",
        "x-amz-credential",
        "aws_secret_access_key",
        "aws_access_key_id",
        "access_token",
        "password",
    ] {
        if let Some(key_start) = find_case_insensitive(&redacted, key) {
            let value_start = key_start + key.len();
            let Some(delimiter) = redacted[value_start..].chars().next() else {
                break;
            };
            if delimiter != '=' && delimiter != ':' {
                break;
            }
            let value_end = redacted[value_start + delimiter.len_utf8()..]
                .find(|character: char| character.is_whitespace() || character == '&')
                .map_or(redacted.len(), |offset| {
                    value_start + delimiter.len_utf8() + offset
                });
            let replacement_start = value_start + delimiter.len_utf8();
            redacted.replace_range(replacement_start..value_end, "<redacted>");
        }
    }

    redacted
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.char_indices().find_map(|(index, _)| {
        haystack
            .get(index..index + needle.len())
            .filter(|candidate| candidate.eq_ignore_ascii_case(needle))
            .map(|_| index)
    })
}

pub type Result<T> = std::result::Result<T, NcvError>;
