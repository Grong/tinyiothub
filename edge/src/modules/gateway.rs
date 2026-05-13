use std::sync::Arc;
use crate::config::{EdgeConfig, GatewayCredentials};

pub struct GatewayService;

impl GatewayService {
    pub fn new(_creds: &GatewayCredentials, _config: &EdgeConfig) -> Arc<Self> {
        Arc::new(Self)
    }
    pub fn credentials(&self) -> &GatewayCredentials {
        unimplemented!()
    }
    pub async fn publish_status(
        &self,
        _payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    pub async fn publish_telemetry(
        &self,
        _payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    pub async fn publish_event(
        &self,
        _payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    pub async fn publish_discovery(
        &self,
        _payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
    pub fn topic_prefix(&self) -> String {
        String::new()
    }
    pub fn is_alive(&self) -> bool {
        true
    }
    pub async fn reconnect(&self) {}
    pub async fn disconnect(&self) {}
}
