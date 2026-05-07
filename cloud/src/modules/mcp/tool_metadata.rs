// IoTToolMetadata — IoT 工具元数据扩展
// 参考 Claude Code Tool 接口中的并发安全和权限属性设计

use serde_json::Value;

/// 权限级别
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PermissionLevel {
    /// 可信操作，自动放行
    Allow,
    /// 需要用户确认
    #[default]
    Ask,
    /// 危险操作，需额外确认
    Deny,
}

/// IoT 工具元数据 trait
/// 为 IoTToolAdapter 提供并发安全和权限属性
pub trait IoTToolMetadata: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 工具描述
    fn description(&self) -> &str;

    /// 输入 JSON Schema
    fn input_schema(&self) -> Value;

    /// 是否并发安全（可并行执行）
    /// 例如：list_devices 读操作可以并发，control_device 写操作不行
    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        false
    }

    /// 是否只读操作
    /// 例如：list_devices 是只读，control_device 不是
    fn is_read_only(&self, _input: &Value) -> bool {
        false
    }

    /// 是否危险操作（删除、固件更新等）
    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    /// 权限级别
    fn permission_level(&self, _input: &Value) -> PermissionLevel {
        PermissionLevel::Ask
    }

    /// 获取工具的安全分类标签（用于日志和调试）
    fn safety_label(&self, input: &Value) -> String {
        let safe = self.is_concurrency_safe(input);
        let readonly = self.is_read_only(input);
        let destructive = self.is_destructive(input);
        let perm = self.permission_level(input);

        let safety = match (safe, readonly, destructive) {
            (true, true, false) => "SAFE_READ",
            (true, false, false) => "SAFE_WRITE",
            (false, true, false) => "UNSAFE_READ",
            (false, false, false) => "UNSAFE_WRITE",
            (_, _, true) => "DESTRUCTIVE",
        };

        let perm_str = match perm {
            PermissionLevel::Allow => "ALLOW",
            PermissionLevel::Ask => "ASK",
            PermissionLevel::Deny => "DENY",
        };

        format!("{}/{}", safety, perm_str)
    }
}

/// 根据工具名称推断并发安全性的辅助函数
pub fn name_infers_concurrency_safe(name: &str) -> bool {
    name.starts_with("list_")
        || name.starts_with("get_")
        || name.starts_with("read_")
        || name.ends_with("_read")
        || name.contains("_query")
        || name.contains("_search")
}

/// 根据工具名称推断是否只读
pub fn name_infers_read_only(name: &str) -> bool {
    name.starts_with("list_")
        || name.starts_with("get_")
        || name.starts_with("read_")
        || name.ends_with("_read")
        || name.ends_with("_query")
        || name.ends_with("_search")
        || name.ends_with("_statistics")
        || name.ends_with("_status")
}

/// 根据工具名称推断是否危险操作
pub fn name_infers_destructive(name: &str) -> bool {
    name.starts_with("delete_")
        || name.starts_with("remove_")
        || name.starts_with("unload_")
        || name.contains("firmware")
        || name.contains("reset")
        || name.contains("reboot")
        || name.contains("factory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrency_safe_inference() {
        assert!(name_infers_concurrency_safe("list_devices"));
        assert!(name_infers_concurrency_safe("get_device_status"));
        assert!(name_infers_concurrency_safe("read_properties"));
        assert!(!name_infers_concurrency_safe("control_device"));
        assert!(!name_infers_concurrency_safe("write_properties"));
    }

    #[test]
    fn test_read_only_inference() {
        assert!(name_infers_read_only("list_devices"));
        assert!(name_infers_read_only("get_device_metrics"));
        assert!(name_infers_read_only("read_properties"));
        assert!(name_infers_read_only("get_device_history"));
        assert!(!name_infers_read_only("create_device"));
        assert!(!name_infers_read_only("update_device"));
    }

    #[test]
    fn test_destructive_inference() {
        assert!(name_infers_destructive("delete_device"));
        assert!(name_infers_destructive("remove_workspace"));
        assert!(name_infers_destructive("unload_driver"));
        assert!(name_infers_destructive("firmware_update"));
        assert!(name_infers_destructive("reset_device"));
        assert!(!name_infers_destructive("create_device"));
    }
}
