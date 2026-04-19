//! Global constants shared across the workspace.

/// Maximum device name length.
pub const MAX_DEVICE_NAME_LEN: usize = 128;

/// Maximum property name length.
pub const MAX_PROPERTY_NAME_LEN: usize = 64;

/// Maximum command name length.
pub const MAX_COMMAND_NAME_LEN: usize = 64;

/// Default page size for paginated queries.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Maximum page size for paginated queries.
pub const MAX_PAGE_SIZE: u32 = 1000;

/// Default request timeout in seconds.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Default database connection pool size.
pub const DEFAULT_DB_POOL_SIZE: u32 = 10;

/// Maximum MQTT message payload size in bytes (1 MB).
pub const MAX_MQTT_PAYLOAD_SIZE: usize = 1_048_576;

/// Default MQTT keep-alive interval in seconds.
pub const DEFAULT_MQTT_KEEP_ALIVE_SECS: u64 = 60;

/// Plugin API version — bump when ABI changes.
pub const PLUGIN_API_VERSION: u32 = 1;

/// Maximum plugin memory limit in bytes (64 MB).
pub const DEFAULT_PLUGIN_MEMORY_LIMIT: u64 = 64 * 1024 * 1024;

/// Session token expiration in seconds (3 hours).
pub const SESSION_EXPIRATION_SECS: u64 = 10_800;
