// GPIO hardware interface
// VC0882 hardware support has been removed

/// Initialize GPIO subsystem
pub fn init() {
    tracing::info!("GPIO subsystem initialized (vc0882 support removed)");
}

/// Set GPIO pin value (stub implementation)
pub fn set_pin(_pin: u32, _value: bool) {
    tracing::debug!("GPIO set_pin called - hardware not available");
}

/// Get GPIO pin value (stub implementation)
pub fn get_pin(_pin: u32) -> bool {
    tracing::debug!("GPIO get_pin called - hardware not available");
    false
}
