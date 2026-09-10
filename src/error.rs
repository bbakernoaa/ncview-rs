use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NcvError {
    #[error("unsupported dataset format for {path}: {reason}")]
    UnsupportedFormat { path: PathBuf, reason: String },
    #[error("invalid dataset {path}: {reason}")]
    InvalidDataset { path: PathBuf, reason: String },
    #[error("unsupported variable {variable}: {reason}")]
    UnsupportedVariable { variable: String, reason: String },
    #[error("invalid slice request: {0}")]
    InvalidSlice(String),
    #[error("terminal capability unavailable: {0}")]
    TerminalCapability(String),
    #[error("worker stopped")]
    WorkerStopped,
    #[error("dataset adapter error for {path}: {reason}")]
    Adapter { path: PathBuf, reason: String },
    #[error("I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, NcvError>;
