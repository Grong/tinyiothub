use crate::config::EdgeConfig;

pub struct PairingClient;

impl PairingClient {
    pub async fn run_pairing(
        _config: &EdgeConfig,
    ) -> Result<crate::config::GatewayCredentials, Box<dyn std::error::Error>> {
        // For now, return an error since pairing needs MQTT broker
        Err("Pairing requires MQTT broker connection — will be wired in Task 11".into())
    }
}
