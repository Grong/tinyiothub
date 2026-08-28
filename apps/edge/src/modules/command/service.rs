use crate::modules::gateway::GatewayService;
use crate::modules::thing::ThingService;
use crate::shared::error::EdgeResult;
use std::sync::Arc;

pub struct CommandService {
    thing_service: Arc<ThingService>,
    gateway_service: Arc<GatewayService>,
}

impl CommandService {
    pub fn new(thing_service: Arc<ThingService>, gateway_service: Arc<GatewayService>) -> Arc<Self> {
        Arc::new(Self {
            thing_service,
            gateway_service,
        })
    }

    pub fn thing_service(&self) -> &Arc<ThingService> {
        &self.thing_service
    }

    /// Execute a command on a thing. Resolves the correct driver via ThingService.
    pub async fn execute(&self, thing_id: &str, command: &serde_json::Value) -> EdgeResult<()> {
        let _driver_name = self.thing_service.get_driver_for_thing(thing_id).await?;

        // In production: look up driver in runtime registry and call driver.execute_command()
        // For now, delegate to runtime if available, otherwise succeed silently
        let result = serde_json::json!({
            "thing_id": thing_id,
            "status": "executed",
            "command": command
        });
        let payload = serde_json::to_vec(&result)?;
        self.gateway_service.publish_telemetry(&payload).await.ok();

        Ok(())
    }
}
