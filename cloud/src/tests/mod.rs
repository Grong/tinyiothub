//! Integration tests module
//!
//! All handler-level integration tests live here as `#[cfg(test)]` modules.
//! They use `tower::ServiceExt::oneshot()` to test HTTP handlers without starting a real server.

mod agent_handler_tests;
mod alarm_handler_tests;
mod auth_handler_tests;
mod batch_handler_tests;
mod chat_handler_tests;
mod cron_handler_tests;
mod device_handler_tests;
mod driver_handler_tests;
mod driver_health_handler_tests;
mod event_handler_tests;
mod jobs_handler_tests;
mod marketplace_handler_tests;
mod mcp_handler_tests;
mod monitoring_handler_tests;
mod notification_handler_tests;
mod open_handler_tests;
mod permission_handler_tests;
mod role_handler_tests;
mod self_healing_handler_tests;
mod system_handler_tests;
mod tag_handler_tests;
mod template_handler_tests;
mod tenant_handler_tests;
mod token_handler_tests;
mod user_handler_tests;
mod workspace_handler_tests;
