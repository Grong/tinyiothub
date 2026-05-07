use std::sync::Arc;

use crate::modules::device::trace_service::DeviceTraceService;

/// 设备追踪工具类
pub struct DeviceTracer {
    trace_service: Arc<DeviceTraceService>,
}

impl DeviceTracer {
    pub fn new(trace_service: Arc<DeviceTraceService>) -> Self {
        Self { trace_service }
    }

    /// 记录操作追踪
    pub async fn trace_operation(
        &self,
        device_id: &str,
        title: &str,
        message: &str,
        user_id: Option<&str>,
        session_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<String, crate::shared::error::Error> {
        self.trace_service
            .record_device_trace(
                device_id,
                "operation",
                "info",
                "user",
                title,
                message,
                details,
                Some("system"),
                user_id,
                session_id,
            )
            .await
    }

    /// 记录错误追踪
    pub async fn trace_error(
        &self,
        device_id: &str,
        title: &str,
        message: &str,
        error_details: Option<serde_json::Value>,
        source: Option<&str>,
    ) -> Result<String, crate::shared::error::Error> {
        self.trace_service
            .record_device_trace(
                device_id,
                "error",
                "error",
                "system",
                title,
                message,
                error_details,
                source,
                None,
                None,
            )
            .await
    }

    /// 记录通信追踪
    pub async fn trace_communication(
        &self,
        device_id: &str,
        title: &str,
        message: &str,
        comm_details: Option<serde_json::Value>,
        duration_ms: Option<u64>,
    ) -> Result<String, crate::shared::error::Error> {
        let mut details = comm_details.unwrap_or_default();
        if let Some(duration) = duration_ms
            && let serde_json::Value::Object(ref mut map) = details
        {
            map.insert("duration_ms".to_string(), serde_json::Value::Number(duration.into()));
        }

        self.trace_service
            .record_device_trace(
                device_id,
                "communication",
                "debug",
                "driver",
                title,
                message,
                Some(details),
                Some("driver"),
                None,
                None,
            )
            .await
    }

    /// 记录性能追踪
    pub async fn trace_performance(
        &self,
        device_id: &str,
        title: &str,
        message: &str,
        metrics: Option<serde_json::Value>,
    ) -> Result<String, crate::shared::error::Error> {
        self.trace_service
            .record_device_trace(
                device_id,
                "performance",
                "info",
                "system",
                title,
                message,
                metrics,
                Some("performance_monitor"),
                None,
                None,
            )
            .await
    }

    /// 记录调试追踪
    pub async fn trace_debug(
        &self,
        device_id: &str,
        title: &str,
        message: &str,
        debug_info: Option<serde_json::Value>,
        source: Option<&str>,
    ) -> Result<String, crate::shared::error::Error> {
        self.trace_service
            .record_device_trace(
                device_id, "debug", "debug", "system", title, message, debug_info, source, None,
                None,
            )
            .await
    }
}

/// 便捷宏，用于快速记录设备追踪
#[macro_export]
macro_rules! trace_device {
    // 操作追踪
    (operation, $tracer:expr, $device_id:expr, $title:expr, $message:expr) => {
        $tracer.trace_operation($device_id, $title, $message, None, None, None).await
    };
    (operation, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $details:expr) => {
        $tracer.trace_operation($device_id, $title, $message, None, None, Some($details)).await
    };
    (operation, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $user_id:expr, $session_id:expr) => {
        $tracer
            .trace_operation($device_id, $title, $message, Some($user_id), Some($session_id), None)
            .await
    };

    // 错误追踪
    (error, $tracer:expr, $device_id:expr, $title:expr, $message:expr) => {
        $tracer.trace_error($device_id, $title, $message, None, None).await
    };
    (error, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $details:expr) => {
        $tracer.trace_error($device_id, $title, $message, Some($details), None).await
    };

    // 通信追踪
    (comm, $tracer:expr, $device_id:expr, $title:expr, $message:expr) => {
        $tracer.trace_communication($device_id, $title, $message, None, None).await
    };
    (comm, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $details:expr) => {
        $tracer.trace_communication($device_id, $title, $message, Some($details), None).await
    };
    (comm, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $details:expr, $duration:expr) => {
        $tracer
            .trace_communication($device_id, $title, $message, Some($details), Some($duration))
            .await
    };

    // 性能追踪
    (perf, $tracer:expr, $device_id:expr, $title:expr, $message:expr) => {
        $tracer.trace_performance($device_id, $title, $message, None).await
    };
    (perf, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $metrics:expr) => {
        $tracer.trace_performance($device_id, $title, $message, Some($metrics)).await
    };

    // 调试追踪
    (debug, $tracer:expr, $device_id:expr, $title:expr, $message:expr) => {
        $tracer.trace_debug($device_id, $title, $message, None, None).await
    };
    (debug, $tracer:expr, $device_id:expr, $title:expr, $message:expr, $details:expr) => {
        $tracer.trace_debug($device_id, $title, $message, Some($details), None).await
    };
}

/// 使用示例
#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_device_tracer_usage() {
        // 这里只是展示用法，实际测试需要真实的DeviceTraceService

        // 假设有一个tracer实例
        // let tracer = DeviceTracer::new(trace_service);

        // 使用宏记录不同类型的追踪

        // 操作追踪
        // trace_device!(operation, tracer, "device_001", "配置更新", "更新采样频率");

        // 错误追踪
        // let error_details = json!({
        //     "error_code": "TIMEOUT",
        //     "timeout_ms": 5000
        // });
        // trace_device!(error, tracer, "device_001", "连接超时", "设备连接超时", error_details);

        // 通信追踪
        // let comm_details = json!({
        //     "protocol": "modbus",
        //     "register": 40001,
        //     "value": 123
        // });
        // trace_device!(comm, tracer, "device_001", "读取寄存器", "成功读取保持寄存器", comm_details, 45);

        // 性能追踪
        // let perf_metrics = json!({
        //     "cpu_usage": 45.2,
        //     "memory_usage": 68.7
        // });
        // trace_device!(perf, tracer, "device_001", "性能监控", "设备性能指标采集", perf_metrics);

        // 调试追踪
        // let debug_info = json!({
        //     "step": "initialization",
        //     "status": "success"
        // });
        // trace_device!(debug, tracer, "device_001", "初始化完成", "设备初始化成功", debug_info);
    }
}
