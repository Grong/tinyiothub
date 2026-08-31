//! TinyIoTHub storage layer
//!
//! `Db` facade (connection pool + per-domain delegate methods) over per-domain
//! modules. All SQL lives in this crate — cloud/edge call `Db` delegates, and
//! CI's SQL residence guard (`sqlx::(query|QueryBuilder|raw_sql)` wordlist)
//! rejects raw SQL elsewhere.
//!
//! ## 设计不变量
//! - 只依赖 core，禁止依赖其他任何 workspace crate
//! - 具体 struct、按领域平铺；各领域文件内 free fn（pub(crate)）+ `impl Db` 委托
//! - 测试使用真实 SQLite（test_helpers::test_pool 直建基线）

/// Agent config/action/dead-letter persistence (agents, agent_configs, agent_tools, agent_actions, agent_dead_letters).
pub mod agent;
/// Agent run reports persistence and row types.
pub mod agent_runs;
/// Alarm + alarm rule persistence and row types.
pub mod alarm;
/// Alarm rule persistence and row types (Task 11 split from alarm.rs).
pub mod alarm_rule;
/// Event audit log persistence (audit_logs, lazily created table).
pub mod audit_log;
/// Auth-owned tables (token blacklist, sms codes, social bindings/configs).
pub mod auth;
/// Batch command persistence (batch_commands / batch_command_items) and row types.
pub mod batch_command;
/// Device cache (in-memory).
pub mod cache;
/// Db connection configuration.
pub mod config;
/// Cron job persistence.
pub mod cron_job;
/// Cron run persistence.
pub mod cron_run;
/// Db facade (connection + domain accessors).
pub mod database;
/// Device command persistence.
pub mod device_command;
/// Device property persistence.
pub mod device_property;
/// Device row mapping helpers.
pub mod device_row_mapper;
/// Driver installation persistence.
pub mod driver_installation;
/// Edge 网关本地持久化（offline_buffer / config_meta，edge 专有表）。
pub mod edge;
/// Db error type.
pub mod error;
/// Event + real-time status persistence and query types.
pub mod event;
/// Heartbeat task/result/trust persistence and row types.
pub mod heartbeat;
/// Agent memory persistence.
pub mod memory;
/// Embedded migrations runner.
pub mod migrations;
/// Shared query model types.
pub mod models;
/// Notification channel persistence.
pub mod notification_channel;
/// Notification rule/history persistence + row types.
pub mod notify;
/// Permission + permission group persistence and row types.
pub mod permission;
/// Autonomy policy persistence + row types.
pub mod policy;
/// Migrating SQLite pool creation (foreign keys on, runs embedded migrations).
pub mod pool;
/// Role persistence and row types.
pub mod role;
/// Two-tier seed module (system + demo), applied at bootstrap.
pub mod seed;
/// Session persistence and row types.
pub mod session;
/// system_settings key-value storage (event security config).
pub mod settings;
/// SQL escaping helpers.
pub mod sql_security;
/// Tag + tag binding persistence and row types.
pub mod tag;
/// Tenant + API key persistence and row types.
pub mod tenant;
/// Thing persistence (things 表唯一入口：Thing 视图 + 原 device.rs 全部内容).
pub mod thing;
/// Thing template persistence (thing_templates / template_categories).
pub mod thing_template;
/// Thing trace persistence (thing_traces) and row types.
pub mod thing_trace;
/// User persistence and row types.
pub mod user;
/// Workspace + knowledge resource persistence and row types.
pub mod workspace;

/// Test helpers: baseline-built in-memory pools (+ seeded fixtures under the
/// `testing` feature).
pub mod test_helpers;

// 公共面显式化（Task 13）：只re-export 跨crate 常用的入口类型；
// 各领域行类型/函数一律经 `tinyiothub_storage::<domain>::...` 模块路径访问。
pub use cache::DeviceCache;
pub use config::DatabaseConfig;
pub use database::Db;
pub use driver_installation::DriverInstallation;
pub use error::{DbError, Result};
pub use models::{Filter, FilterOp, Pagination, RowMetadata, SortOrder};
pub use pool::{create_pool, create_pool_without_migrations};
