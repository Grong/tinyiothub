//! E4a driver acceptance: profile definitions, reports and the sync runner.

pub mod profiles;
pub mod report;
pub mod runner;
pub mod test_hub;

pub use report::{AcceptanceReport, CheckResult, Level, ProfileReport};
pub use runner::run_profiles;
