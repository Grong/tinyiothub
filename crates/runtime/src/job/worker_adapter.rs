use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing_command::ThingCommand;

use crate::arbitration::ResourceLease;
use crate::data_server::DataServer;
use crate::driver::worker::WorkerHandle;

pub trait JobDeviceWorker: Send + Sync {
    fn submit(&self, cmd: ThingCommand, job_id: &str, lease: Option<ResourceLease>) -> Result<(), Error>;
    fn cancel_job(&self, job_id: &str) -> bool;
    fn has_recoverable_call(&self, job_id: &str) -> bool;
    fn complete_job(&self, job_id: &str);
}

pub trait JobWorkerLocator: Send + Sync {
    fn worker_for(&self, device_id: &str) -> Option<Arc<dyn JobDeviceWorker>>;
}

#[async_trait]
pub trait JobResultSink: Send + Sync {
    async fn command_finished(&self, driver_call_id: &str, success: bool, error: Option<&str>);
    async fn command_cancelled(&self, job_id: &str, within_grace: bool);
}

#[derive(Clone)]
pub struct DataServerJobWorkers {
    data_server: DataServer,
}

impl DataServerJobWorkers {
    pub fn new(data_server: DataServer) -> Self {
        Self { data_server }
    }
}

impl JobWorkerLocator for DataServerJobWorkers {
    fn worker_for(&self, device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
        self.data_server
            .workers()
            .get(device_id)
            .map(|worker| Arc::new(WorkerJobAdapter { worker }) as Arc<dyn JobDeviceWorker>)
    }
}

struct WorkerJobAdapter {
    worker: Arc<WorkerHandle>,
}

impl JobDeviceWorker for WorkerJobAdapter {
    fn submit(&self, cmd: ThingCommand, job_id: &str, lease: Option<ResourceLease>) -> Result<(), Error> {
        self.worker.submit_job(cmd, job_id, lease)
    }

    fn cancel_job(&self, job_id: &str) -> bool {
        self.worker.cancel_job(job_id)
    }

    fn has_recoverable_call(&self, job_id: &str) -> bool {
        self.worker.has_job(job_id)
    }

    fn complete_job(&self, job_id: &str) {
        self.worker.complete_job(job_id);
    }
}
