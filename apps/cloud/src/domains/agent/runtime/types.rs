#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeFlags {
    pub tasks: bool,
    pub media: bool,
    pub workbench: bool,
}

impl RuntimeFlags {
    pub fn from_heartbeat_json(value: &serde_json::Value) -> Self {
        Self {
            tasks: value
                .get("runtime_tasks_enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            media: value
                .get("runtime_media_enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            workbench: value
                .get("runtime_workbench_enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }
    }
}
