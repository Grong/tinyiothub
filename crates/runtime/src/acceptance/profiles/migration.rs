//! `migration` profile: v1 packages are rejected with actionable guidance and
//! ABI v2 version negotiation succeeds.

use tinyiothub_core::driver::dynamic::{v1_unsupported_message, verify_v2_version};

use crate::acceptance::report::{CheckResult, Level, ProfileReport};

const PROFILE: &str = "migration";

pub fn run() -> ProfileReport {
    let msg = v1_unsupported_message(std::path::Path::new("libv1.so"));
    let checks = vec![
        CheckResult {
            profile: PROFILE.into(),
            check: "v1.rejected".into(),
            level: if msg.contains("tinyiothub_driver_vtable_v2") {
                Level::Pass
            } else {
                Level::Fail
            },
            detail: msg,
            evidence: serde_json::Value::Null,
        },
        CheckResult {
            profile: PROFILE.into(),
            check: "v2.version".into(),
            level: if verify_v2_version(2).is_ok() {
                Level::Pass
            } else {
                Level::Fail
            },
            detail: "ABI v2 accepted".into(),
            evidence: serde_json::Value::Null,
        },
    ];
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_profile_passes() {
        let report = run();
        assert_eq!(report.level(), Level::Pass);
        assert_eq!(report.checks.len(), 2);
    }
}
