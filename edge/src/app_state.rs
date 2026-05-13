use std::sync::Arc;
use crate::config::{EdgeConfig, GatewayCredentials};
use crate::shared::storage::init_database;
use crate::modules::gateway::GatewayService;
use crate::modules::device::DeviceService;
use crate::modules::driver::DriverService;
use crate::modules::telemetry::TelemetryService;
use crate::modules::command::CommandService;
use crate::modules::config_mgmt::ConfigService;
use crate::modules::health::HealthService;
use crate::modules::intelligence::IntelligenceService;
use crate::modules::offline::OfflineBuffer;

pub struct AppState {
    pub config: EdgeConfig,
    pub credentials: GatewayCredentials,
    pub db: Arc<tinyiothub_storage::sqlite::Database>,
    pub device_service: Arc<DeviceService>,
    pub driver_service: Arc<DriverService>,
    pub gateway_service: Arc<GatewayService>,
    pub telemetry_service: Arc<TelemetryService>,
    pub command_service: Arc<CommandService>,
    pub config_service: Arc<ConfigService>,
    pub health_service: Arc<HealthService>,
    pub intelligence_service: Arc<IntelligenceService>,
    pub offline_buffer: Arc<OfflineBuffer>,
}

impl AppState {
    pub async fn new(
        config: EdgeConfig,
        credentials: GatewayCredentials,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Layer 1: No dependencies
        let db = init_database(&config.db_path.to_string_lossy()).await?;
        let offline_buffer = OfflineBuffer::new(db.clone(), config.clone());

        // Layer 2: Depends on Layer 1
        let device_service = DeviceService::new(db.clone());
        let gateway_service = GatewayService::new(&credentials, &config);
        let driver_service = DriverService::new(db.clone(), config.scan_timeout_secs);

        // Layer 3: Depends on Layer 2
        let telemetry_service = TelemetryService::new(
            driver_service.clone(),
            gateway_service.clone(),
            offline_buffer.clone(),
        );
        let command_service =
            CommandService::new(device_service.clone(), gateway_service.clone());
        let config_service = ConfigService::new(db.clone(), config.clone(), gateway_service.clone());
        let health_service = HealthService::new(
            gateway_service.clone(),
            offline_buffer.clone(),
            driver_service.clone(),
        );
        let intelligence_service = IntelligenceService::new(
            device_service.clone(),
            driver_service.clone(),
            gateway_service.clone(),
        );

        Ok(Self {
            config,
            credentials,
            db,
            device_service,
            driver_service,
            gateway_service,
            telemetry_service,
            command_service,
            config_service,
            health_service,
            intelligence_service,
            offline_buffer,
        })
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            credentials: self.credentials.clone(),
            db: self.db.clone(),
            device_service: self.device_service.clone(),
            driver_service: self.driver_service.clone(),
            gateway_service: self.gateway_service.clone(),
            telemetry_service: self.telemetry_service.clone(),
            command_service: self.command_service.clone(),
            config_service: self.config_service.clone(),
            health_service: self.health_service.clone(),
            intelligence_service: self.intelligence_service.clone(),
            offline_buffer: self.offline_buffer.clone(),
        }
    }
}
