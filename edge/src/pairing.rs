use rand::Rng;
use std::time::{Duration, Instant};

const PAIRING_CODE_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

pub struct PairingCodeGenerator {
    current_code: String,
    last_refresh: Instant,
}

impl PairingCodeGenerator {
    pub fn new() -> Self {
        Self {
            current_code: Self::generate_code(),
            last_refresh: Instant::now(),
        }
    }

    fn generate_code() -> String {
        let mut rng = rand::thread_rng();
        let code: u32 = rng.gen_range(0..1_000_000);
        format!("{:06}", code)
    }

    pub fn get_code(&mut self) -> &str {
        if self.last_refresh.elapsed() >= PAIRING_CODE_REFRESH_INTERVAL {
            self.current_code = Self::generate_code();
            self.last_refresh = Instant::now();
            tracing::info!(code = %self.current_code, "Pairing code refreshed");
        }
        &self.current_code
    }

    pub fn display_format(code: &str) -> String {
        format!("{} {}", &code[..3], &code[3..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_is_six_digits() {
        let mut generator = PairingCodeGenerator::new();
        let code = generator.get_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_same_code_within_ttl() {
        let mut generator = PairingCodeGenerator::new();
        let code1 = generator.get_code().to_string();
        let code2 = generator.get_code().to_string();
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_display_format() {
        assert_eq!(PairingCodeGenerator::display_format("482916"), "482 916");
    }
}
