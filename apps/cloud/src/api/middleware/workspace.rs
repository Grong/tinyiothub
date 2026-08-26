// Compat shim — canonical home is tinyiothub_web::middleware::workspace
// (P4-Task15). The extractor resolves the workspace from the tenant resolver
// registered by the cloud binary at startup (same validate_jwt path as
// before), so behavior is unchanged for existing importers.
pub use tinyiothub_web::middleware::workspace::WorkspaceScope;
