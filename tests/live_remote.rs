//! Opt-in live-provider smoke checks.
//!
//! These tests intentionally do nothing unless a source URI is supplied. The
//! URI is read from the environment rather than committed or printed, and
//! failures are reported through the application's redacted error type.

use std::env;

use ncview_rs::data;

#[test]
fn optional_live_provider_smoke() {
    for variable in ["NCVIEW_LIVE_S3", "NCVIEW_LIVE_GCS", "NCVIEW_LIVE_AZURE"] {
        let Ok(location) = env::var(variable) else {
            continue;
        };
        let source = data::open_location(&location)
            .unwrap_or_else(|error| panic!("{variable} live smoke failed: {error}"));
        assert!(source.is_remote(), "{variable} must contain a remote URI");
        assert!(source.source_identity().is_some());
        assert!(!source.metadata().variables.is_empty());
    }
}
