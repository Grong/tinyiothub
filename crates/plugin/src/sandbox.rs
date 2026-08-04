//! Plugin sandbox configuration for resource limits.

/// Per-plugin resource limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxLimits {
    /// Max heap memory in bytes (0 = unlimited).
    pub max_memory_bytes: u64,
    /// Max CPU time per call in milliseconds (0 = unlimited).
    pub max_cpu_time_ms: u64,
    /// Max open file descriptors.
    pub max_open_files: u32,
    /// Max concurrent threads.
    pub max_threads: u32,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_cpu_time_ms: 5_000,             // 5 seconds
            max_open_files: 16,
            max_threads: 2,
        }
    }
}

/// Sandbox configuration for the plugin runtime.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub default_limits: SandboxLimits,
    pub per_plugin_limits: std::collections::HashMap<String, SandboxLimits>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_limits: SandboxLimits::default(),
            per_plugin_limits: std::collections::HashMap::new(),
        }
    }
}

impl SandboxConfig {
    pub fn limits_for(&self, plugin_name: &str) -> SandboxLimits {
        self.per_plugin_limits
            .get(plugin_name)
            .copied()
            .unwrap_or(self.default_limits)
    }
}
