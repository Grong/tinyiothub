//! Driver access layer: package loading, manifest registry, materialization.

pub mod catalog;
pub mod demand;
pub mod device;
pub mod materialize;
pub mod package;
pub mod registry;
pub mod service;
pub mod validator;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("missing driver.manifest.json in {0}")]
    MissingManifest(PathBuf),
    #[error("manifest error: {0}")]
    Manifest(#[from] tinyiothub_core::driver::manifest::ManifestError),
    #[error("manifest JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("driver '{0}' not installed in workspace '{1}'")]
    NotFound(String, String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("ABI error: {0}")]
    Abi(String),
    #[error("store error: {0}")]
    Store(String),
}
