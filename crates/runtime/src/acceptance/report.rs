//! Acceptance report shapes. Two verdicts per profile: individual checks carry
//! five levels, while `passed()` aggregates to pass/fail.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Pass,
    Fail,
    Warn,
    Skip,
    Manual,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Pass => "pass",
            Level::Fail => "fail",
            Level::Warn => "warn",
            Level::Skip => "skip",
            Level::Manual => "manual",
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub profile: String,
    pub check: String,
    pub level: Level,
    pub detail: String,
    #[serde(default)]
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileReport {
    pub profile: String,
    pub checks: Vec<CheckResult>,
}

impl ProfileReport {
    pub fn level(&self) -> Level {
        if self.checks.iter().any(|c| c.level == Level::Fail) {
            Level::Fail
        } else if self.checks.iter().any(|c| c.level == Level::Warn) {
            Level::Warn
        } else if self.checks.iter().all(|c| c.level == Level::Skip) {
            Level::Skip
        } else if self.checks.iter().any(|c| c.level == Level::Pass) {
            Level::Pass
        } else {
            Level::Manual
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub package: String,
    pub driver: String,
    pub version: String,
    pub profiles: Vec<ProfileReport>,
}

impl AcceptanceReport {
    /// `warn` does not fail acceptance; only a profile-level `fail` does.
    pub fn passed(&self) -> bool {
        self.profiles.iter().all(|p| p.level() != Level::Fail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(level: Level) -> CheckResult {
        CheckResult {
            profile: "p".into(),
            check: "c".into(),
            level,
            detail: String::new(),
            evidence: serde_json::Value::Null,
        }
    }

    fn profile(levels: &[Level]) -> ProfileReport {
        ProfileReport {
            profile: "p".into(),
            checks: levels.iter().copied().map(check).collect(),
        }
    }

    #[test]
    fn fail_dominates_every_other_level() {
        assert_eq!(profile(&[Level::Pass, Level::Fail, Level::Warn]).level(), Level::Fail);
    }

    #[test]
    fn warn_dominates_pass_but_does_not_block_acceptance() {
        let report = AcceptanceReport {
            package: "/pkg".into(),
            driver: "d".into(),
            version: "1.0.0".into(),
            profiles: vec![profile(&[Level::Pass, Level::Warn])],
        };
        assert_eq!(report.profiles[0].level(), Level::Warn);
        assert!(report.passed(), "warn must not block acceptance");
    }

    #[test]
    fn manual_and_skip_profiles_pass_acceptance() {
        let report = AcceptanceReport {
            package: "/pkg".into(),
            driver: "d".into(),
            version: "1.0.0".into(),
            profiles: vec![profile(&[Level::Manual]), profile(&[Level::Skip, Level::Skip])],
        };
        assert_eq!(report.profiles[0].level(), Level::Manual);
        assert_eq!(report.profiles[1].level(), Level::Skip);
        assert!(report.passed());
    }

    #[test]
    fn any_failing_profile_fails_the_report() {
        let report = AcceptanceReport {
            package: "/pkg".into(),
            driver: "d".into(),
            version: "1.0.0".into(),
            profiles: vec![profile(&[Level::Pass]), profile(&[Level::Fail])],
        };
        assert!(!report.passed());
    }
}
