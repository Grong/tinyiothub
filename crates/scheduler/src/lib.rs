//! TinyIoTHub cron engine and scheduler service.
//!
//! Modules:
//! - `engine`    — Job executor registry and built-in executors (shell, agent)
//! - `scheduler` — CronSchedulerService: polls for due jobs and executes them
//!
//! The crate depends only on `core` contracts (executor traits, repository
//! traits, models). Concrete db-bound executors (device command, event
//! retention) live in `tinyiothub_runtime::cron_executors` and are wired into
//! the registry by the application (cloud service manager).
//!
//! ## 设计不变量
//! - 不依赖任何领域 crate；调度任务经注入的执行器运行

pub mod engine;
pub mod scheduler;

pub use engine::{AgentExecutor, ExecutionResult, ExecutorError, ExecutorRegistry, JobExecutor, ShellExecutor};
pub use scheduler::CronSchedulerService;
