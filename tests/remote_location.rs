use std::path::Path;

use ncview_rs::error::NcvError;
use ncview_rs::storage::location::{Provider, SourceLocation};

#[test]
fn parses_local_paths_without_treating_them_as_objects() {
    let location = SourceLocation::parse("../data/sample.nc").unwrap();

    assert_eq!(location.provider(), Provider::Local);
    assert_eq!(location.local_path(), Some(Path::new("../data/sample.nc")));
    assert!(location.container().is_none());
    assert_eq!(location.object_key(), "");
    assert_eq!(location.to_string(), "../data/sample.nc");
}

#[test]
fn parses_supported_cloud_forms() {
    let cases = [
        (
            "s3://bucket/path/file.nc",
            Provider::S3,
            "bucket",
            "path/file.nc",
        ),
        (
            "gs://bucket/path/file.grib2",
            Provider::Gcs,
            "bucket",
            "path/file.grib2",
        ),
        (
            "az://container/path/file.nc",
            Provider::Azure,
            "container",
            "path/file.nc",
        ),
        (
            "abfs://container@account.blob.core.windows.net/path/file.nc",
            Provider::Azure,
            "container",
            "path/file.nc",
        ),
        (
            "abfss://container@account.dfs.core.windows.net/path/file.nc",
            Provider::Azure,
            "container",
            "path/file.nc",
        ),
    ];

    for (raw, provider, container, key) in cases {
        let location = SourceLocation::parse(raw).unwrap();
        assert_eq!(location.provider(), provider, "{raw}");
        assert_eq!(location.container(), Some(container), "{raw}");
        assert_eq!(location.object_key(), key, "{raw}");
        assert_eq!(location.to_string(), raw, "{raw}");
    }
}

#[test]
fn rejects_invalid_remote_locations() {
    for raw in [
        "s3://bucket/",
        "s3:///path/file.nc",
        "s3://bucket/path/../secret.nc",
        "s3://bucket/path/*/file.nc",
        "ftp://host/file.nc",
        "s3://bucket/path/file.nc?X-Amz-Signature=secret",
        "s3://bucket/path/\nfile.nc",
        "az://container@/path/file.nc",
    ] {
        assert!(
            SourceLocation::parse(raw).is_err(),
            "accepted invalid location {raw:?}"
        );
    }
}

#[test]
fn remote_diagnostics_redact_credentials_signed_queries_and_headers() {
    let source = SourceLocation::parse("s3://public-bucket/path/data.nc").unwrap();
    let secrets = [
        "https://bucket/path?X-Amz-Signature=signature-secret",
        "Authorization: Bearer bearer-secret",
        "aws_secret_access_key=access-secret",
    ];

    for secret in secrets {
        let error = NcvError::remote_failure(source.clone(), "range", secret);
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains(secret), "secret leaked in {rendered}");
        assert!(
            !rendered.contains("signature-secret"),
            "signed query leaked in {rendered}"
        );
        assert!(
            !rendered.contains("bearer-secret"),
            "authorization header leaked in {rendered}"
        );
        assert!(
            !rendered.contains("access-secret"),
            "credential leaked in {rendered}"
        );
    }
}

#[test]
fn cli_rejects_remote_globs_before_entering_the_terminal() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ncv"))
        .arg("s3://bucket/path/*.grib2")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("remote object globs are not supported"));
}
