// crates/tinyiothub-runtime/src/driver/validation.rs

use tinyiothub_core::error::Error;

/// Validate a driver name to prevent path traversal and reserved names.
pub fn validate_driver_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::ValidationError("driver_name cannot be empty".into()));
    }
    if name.len() > 64 {
        return Err(Error::ValidationError("driver_name too long (max 64)".into()));
    }
    let re = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    if !re.is_match(name) {
        return Err(Error::ValidationError(
            "driver_name contains invalid characters (allowed: a-z A-Z 0-9 _ -)".into(),
        ));
    }
    let reserved = [".", "..", "builtin", "system", "default"];
    if reserved.contains(&name) {
        return Err(Error::ValidationError(format!("driver_name '{}' is reserved", name)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_driver_names() {
        assert!(validate_driver_name("modbus").is_ok());
        assert!(validate_driver_name("my-driver_v2").is_ok());
        assert!(validate_driver_name("Sensor123").is_ok());
    }

    #[test]
    fn test_invalid_driver_names() {
        assert!(validate_driver_name("").is_err());
        assert!(validate_driver_name("../../etc/passwd").is_err());
        assert!(validate_driver_name("builtin").is_err());
        assert!(validate_driver_name("system").is_err());
        assert!(validate_driver_name(&"a".repeat(65)).is_err());
    }
}
