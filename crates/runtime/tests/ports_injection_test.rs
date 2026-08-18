//! Port-injection equivalence tests (D15, Task 11.5).
//!
//! In-memory fakes implement the `ports` traits; the executors under test
//! must behave exactly as they did against the concrete db types: same
//! lookups, same call order, same outputs/errors.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tinyiothub_core::cron::{ExecutorError, JobExecutor};
use tinyiothub_core::models::cron_job::CronJob;
use tinyiothub_core::models::device::Device;
use tinyiothub_core::models::device_command::DeviceCommand;
use tinyiothub_runtime::cron_executors::{DeviceCommandExecutor, EventRetentionExecutor};
use tinyiothub_runtime::data_server::DataServer;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::{DeviceCacheSource, DeviceCommandQueries, EventRetentionStore};

struct FakeCache {
    devices: Mutex<Vec<Device>>,
}

impl FakeCache {
    fn with_device(device: Device) -> Self {
        Self { devices: Mutex::new(vec![device]) }
    }
}

impl DeviceCacheSource for FakeCache {
    fn all(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
    }

    fn get(&self, id: &str) -> Option<Device> {
        self.devices.lock().unwrap().iter().find(|d| d.id == id).cloned()
    }

    fn get_by_name(&self, name: &str) -> Option<Device> {
        self.devices.lock().unwrap().iter().find(|d| d.name == name).cloned()
    }

    fn insert(&self, device: Device) {
        self.devices.lock().unwrap().push(device);
    }

    fn update(&self, device: Device) {
        let mut devices = self.devices.lock().unwrap();
        if let Some(existing) = devices.iter_mut().find(|d| d.id == device.id) {
            *existing = device;
        }
    }

    fn remove(&self, id: &str) {
        self.devices.lock().unwrap().retain(|d| d.id != id);
    }
}

struct FakeCommands {
    command: Option<DeviceCommand>,
}

#[async_trait]
impl DeviceCommandQueries for FakeCommands {
    async fn find_by_device_and_name(&self, device_id: &str, name: &str) -> Result<Option<DeviceCommand>, String> {
        Ok(self
            .command
            .as_ref()
            .filter(|c| c.device_id == device_id && c.name == name)
            .cloned())
    }
}

struct FakeRetention {
    deleted: u64,
    last_cutoff: Mutex<Option<String>>,
}

#[async_trait]
impl EventRetentionStore for FakeRetention {
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        *self.last_cutoff.lock().unwrap() = Some(cutoff_rfc3339.to_string());
        Ok(self.deleted)
    }
}

fn device_command_job(device_id: &str, command_name: &str) -> CronJob {
    CronJob {
        id: "test-cmd".to_string(),
        name: "test".to_string(),
        description: None,
        job_type: "device_command".to_string(),
        cron_expression: "0 0 * * * *".to_string(),
        config: format!(r#"{{"device_id": "{}", "command_name": "{}"}}"#, device_id, command_name),
        timeout_seconds: 300,
        max_retries: 3,
        is_enabled: true,
        is_running: false,
        last_run_at: None,
        last_run_status: None,
        last_run_error: None,
        next_run_at: None,
        run_count: 0,
        success_count: 0,
        fail_count: 0,
        created_at: "2026-08-18 00:00:00".to_string(),
        updated_at: "2026-08-18 00:00:00".to_string(),
        created_by: None,
        workspace_id: None,
    }
}

fn retention_job(retention_days: i64) -> CronJob {
    CronJob {
        job_type: "event_retention".to_string(),
        config: format!(r#"{{"retention_days": {}}}"#, retention_days),
        ..device_command_job("unused", "unused")
    }
}

fn test_data_server() -> Arc<DataServer> {
    let cache = Arc::new(FakeCache::with_device(Device {
        id: "dev-1".to_string(),
        name: "device-one".to_string(),
        ..Default::default()
    }));
    Arc::new(DataServer::new(cache, Arc::new(EventBus::new())))
}

#[tokio::test]
async fn device_command_executor_queues_command_via_port() {
    let executor = DeviceCommandExecutor::new(
        test_data_server(),
        Arc::new(FakeCommands {
            command: Some(DeviceCommand {
                id: "cmd-1".to_string(),
                device_id: "dev-1".to_string(),
                name: "reboot".to_string(),
                display_name: None,
                description: None,
                parameters: None,
                created_at: "2026-08-18 00:00:00".to_string(),
            }),
        }),
    );

    let result = executor
        .execute(&device_command_job("dev-1", "reboot"), "run-1")
        .await
        .expect("execute");
    assert_eq!(result.status, "success");
    assert!(result.output.unwrap().contains("queued for execution"));
}

#[tokio::test]
async fn device_command_executor_errors_when_command_not_found() {
    let executor = DeviceCommandExecutor::new(test_data_server(), Arc::new(FakeCommands { command: None }));

    let err = executor
        .execute(&device_command_job("dev-1", "reboot"), "run-1")
        .await
        .expect_err("must fail when the port returns None");
    match err {
        ExecutorError::InvalidConfig(msg) => assert!(msg.contains("not found")),
        other => panic!("expected InvalidConfig, got {:?}", other),
    }
}

#[tokio::test]
async fn event_retention_executor_delegates_cutoff_to_port() {
    let store = Arc::new(FakeRetention {
        deleted: 7,
        last_cutoff: Mutex::new(None),
    });
    let executor = EventRetentionExecutor::new(store.clone());

    let result = executor.execute(&retention_job(30), "run-1").await.expect("execute");
    assert!(result.output.unwrap().contains("deleted 7 "));

    let cutoff = store.last_cutoff.lock().unwrap().clone().expect("port called");
    // 30-day retention ⇒ cutoff is roughly now minus 30 days (RFC3339).
    let cutoff_dt = chrono::DateTime::parse_from_rfc3339(&cutoff).expect("rfc3339 cutoff");
    let age = chrono::Utc::now() - cutoff_dt.with_timezone(&chrono::Utc);
    assert!(age.num_days() >= 29 && age.num_days() <= 31, "cutoff age {}d", age.num_days());
}
