use crate::shared::error::Error;

// All vc0882-specific hardware functions have been removed
// as the vc0882 device is no longer supported

/// Placeholder for future hardware abstraction layer
pub fn hardware_init() -> Result<(), Error> {
    tracing::info!("Hardware abstraction layer initialized (vc0882 support removed)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_init_returns_ok() {
        let result = hardware_init();
        assert!(result.is_ok());
    }
}
