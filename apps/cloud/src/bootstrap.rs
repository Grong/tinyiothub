//! 启动引导逻辑 —— 从 `main.rs` 拆出，main 只做组装（P5-Task25）。
//!
//! 包含：日志初始化、动态驱动重载、设备缓存预热、agent 子系统启动恢复
//! （Task 9：快照装配 + 僵尸 run reconcile）。

use tracing::{info, warn};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;
use tinyiothub_core::config::ApplicationSettings;

/// Set up global panic handler to prevent crashes
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic message".to_string()
        };

        eprintln!("🚨 PANIC CAUGHT: {} at {}", message, location);
        eprintln!("Application will continue running...");

        // Log to tracing if available
        tracing::error!("PANIC: {} at {}", message, location);
    }));
}

/// Initialize the logging system based on configuration
pub async fn initialize_logging(config: &ApplicationSettings) -> std::io::Result<()> {
    // Declare _guard variable to retain WorkerGuard for main function lifetime
    let _guard;

    // Create log directory if it doesn't exist
    if config.logging.file_enabled
        && let Some(parent) = config.log_file_path().parent()
    {
        std::fs::create_dir_all(parent)?;
    }

    if config.logging.file_enabled {
        info!(
            "File logging enabled (level: {}, path: {:?})",
            config.logging.level,
            config.log_file_path()
        );

        // Console log layer
        let console_layer = fmt::layer().with_ansi(true).with_writer(std::io::stderr);

        // Create rolling file appender
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("app")
            .filename_suffix("log")
            .max_log_files(config.logging.max_files as usize)
            .build(
                config
                    .log_file_path()
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("logs")),
            )
            .unwrap();

        // Create non-blocking writer
        let (non_blocking, guard) = non_blocking(file_appender);
        _guard = guard;

        // File log layer (disable ANSI colors)
        let file_layer = fmt::layer().with_ansi(false).with_writer(non_blocking);

        // Create filter layer
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&config.logging.level))
            .expect("Cannot initialize log filter");

        // Register global subscriber
        tracing_subscriber::registry()
            .with(console_layer)
            .with(filter_layer)
            .with(file_layer)
            .init();
    } else {
        // Console logging only
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&config.logging.level))
            .expect("Cannot initialize log filter");

        tracing_subscriber::fmt().with_env_filter(filter_layer).init();

        info!("Console logging only (level: {})", config.logging.level);
    }

    Ok(())
}

/// 重新加载已安装的动态驱动
pub async fn rehydrate_drivers(app_state: &AppState) {
    match app_state.db.find_all_driver_installations().await {
        Ok(installations) => {
            let registry = tinyiothub_runtime::driver_registry();
            for inst in installations {
                let path = std::path::PathBuf::from(&inst.file_path);
                match registry.write().load(&path, &inst.workspace_id) {
                    Ok(name) => {
                        info!("✅ Rehydrated driver '{}' for workspace {}", name, inst.workspace_id)
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to rehydrate driver {}: {}", inst.driver_name, e)
                    }
                }
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to load driver installations: {}", e);
        }
    }
}

/// 启动种子（Task 3 两档 seed 模块）：`Db::connect` 之后调用。
/// 系统档（租户/工作区/admin/内置模板等生产必需行）始终应用；
/// 演示档受 `[seed] demo_data` 开关控制（默认 true）。
pub async fn run_seeds(db: &tinyiothub_storage::Db, settings: &ApplicationSettings) -> Result<(), sqlx::Error> {
    tinyiothub_storage::seed::seed_system(db).await?;
    if settings.seed.demo_data {
        tinyiothub_storage::seed::seed_demo(db).await?;
    } else {
        info!("Demo seed data disabled ([seed] demo_data = false)");
    }
    Ok(())
}

/// 从数据库加载完整设备（含属性、指令）到缓存
pub async fn load_device_cache(app_state: &AppState) {
    use tinyiothub_core::models::device::DeviceQueryParams;
    match app_state
        .device_service
        .get_devices(&DeviceQueryParams::default())
        .await
    {
        Ok(devices) => {
            let device_ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
            let count = device_ids.len();
            match app_state.device_service.load_complete_devices(&device_ids).await {
                Ok(complete_devices) => {
                    for device in complete_devices {
                        app_state.device_cache.insert(device);
                    }
                    info!("✅ Loaded {} complete devices (with properties) into cache", count);
                }
                Err(e) => {
                    warn!("⚠️ Failed to load complete devices, falling back to basic: {}", e);
                    for device in devices {
                        app_state.device_cache.insert(device);
                    }
                }
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to load devices into cache: {}", e);
        }
    }
}

// ── Agent 子系统启动恢复（Task 9）──────────────────────────────────
//
// 启动顺序（D11-①③，错序即丢事件）由 service_manager 编排：
//   1. build_agent_snapshot  —— 从 DB 装配 RestoreSnapshot
//   2. bus 先建并经 RuntimeDeps 注入 restore；持久化 receiver 在 restore
//      之前从该 bus 取得（"先 subscribe 再 restore"的可实现形式）
//   3. reconcile_zombie_runs —— 僵尸 run 标记 'interrupted'
//   4. spawn 持久化订阅者（restore 前取得的 receiver + shutdown token）

use sqlx::SqlitePool;
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_storage::Db;
use tinyiothub_storage::agent_runs::AgentRunsRepository;
use tinyiothub_storage::heartbeat::HeartbeatTaskRepository;

use tinyiothub_agent::runtime::heartbeat::types::HeartbeatConfig;
use tinyiothub_agent::runtime::runtime::AgentRuntime;
use tinyiothub_agent::runtime::snapshot::{ProblemMetaRow, RestoreSnapshot, WorkspaceHeartbeatState};
use tinyiothub_agent::runtime::thing_agent::registry::COMPLETED_CAPACITY;

/// 启动顺序第 1 步：从 DB 装配 AgentRuntime 恢复快照。
///
/// - heartbeat 段：每工作区 tasks + trust config + interval（缺省回退
///   `HeartbeatConfig::default()`，与 runner 缺省一致）；
/// - recent_runs 段：每工作区最近 [`COMPLETED_CAPACITY`] 条已完成 run，
///   **旧→新**（顺序契约见 `RestoreSnapshot::recent_runs` —— RunReport
///   无时间戳，registry 无法自排序）；
/// - problem_meta 段（Task 6 遗留指针）：agent_runs 7d 保留窗内的
///   problem_key 行直接查询装配（problem_key/outcome/verified/acked_at/
///   created_at 列齐全；core RunReport 无这些字段），否则重启后 dedup
///   状态为空，近期已处理问题会重复派发一次。
///
/// 单项失败降级为空段 + warn（启动不阻塞）；DB 不可达时返回空快照。
pub async fn build_agent_snapshot(db: &Db) -> RestoreSnapshot {
    let pool = db.pool();
    let ws_ids = match db.find_all_workspace_ids().await {
        Ok(ids) => ids,
        Err(e) => {
            warn!(error = %e, "agent snapshot: list workspace ids failed, restoring empty");
            return RestoreSnapshot::default();
        }
    };

    let task_repo = HeartbeatTaskRepository::new(pool.clone());
    let default_interval = HeartbeatConfig::default().interval_minutes;
    let mut heartbeat = Vec::with_capacity(ws_ids.len());
    let mut recent_runs = Vec::new();
    for ws_id in &ws_ids {
        let tasks = task_repo.list_by_workspace(ws_id).await.unwrap_or_else(|e| {
            warn!(workspace_id = %ws_id, error = %e, "agent snapshot: load heartbeat tasks failed");
            vec![]
        });
        let trust_config = task_repo
            .load_trust_config(ws_id)
            .await
            .unwrap_or_else(|e| {
                warn!(workspace_id = %ws_id, error = %e, "agent snapshot: load trust config failed");
                None
            })
            .unwrap_or_default();
        let interval_minutes = task_repo
            .load_heartbeat_config(ws_id)
            .await
            .unwrap_or_else(|e| {
                warn!(workspace_id = %ws_id, error = %e, "agent snapshot: load heartbeat config failed");
                None
            })
            .map(|c| c.interval_minutes)
            .unwrap_or(default_interval);
        heartbeat.push(WorkspaceHeartbeatState {
            workspace_id: ws_id.clone(),
            tasks,
            trust_config,
            interval_minutes,
        });
        recent_runs.extend(load_recent_runs(pool, ws_id).await);
    }
    let problem_meta = load_problem_meta(pool).await;
    RestoreSnapshot {
        heartbeat,
        recent_runs,
        problem_meta,
    }
}

/// 每工作区最近 [`COMPLETED_CAPACITY`] 条 run，**旧→新**（prewarm 输入
/// 契约）：内层取最新 N 条，外层翻转。report JSON 解析失败的行跳过。
async fn load_recent_runs(pool: &SqlitePool, workspace_id: &str) -> Vec<RunReport> {
    let rows: Vec<(String,)> = match sqlx::query_as(
        "SELECT report FROM (
             SELECT report, created_at, rowid AS rid FROM agent_runs
             WHERE workspace_id = ?
             ORDER BY created_at DESC, rid DESC
             LIMIT ?
         ) ORDER BY created_at ASC, rid ASC",
    )
    .bind(workspace_id)
    .bind(COMPLETED_CAPACITY as i64)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!(workspace_id = %workspace_id, error = %e, "agent snapshot: load recent runs failed");
            return vec![];
        }
    };
    rows.into_iter()
        .filter_map(|(json,)| match serde_json::from_str::<RunReport>(&json) {
            Ok(report) => Some(report),
            Err(e) => {
                warn!(workspace_id = %workspace_id, error = %e, "agent snapshot: skip unparseable run report");
                None
            }
        })
        .collect()
}

/// O11 dedup 元数据段：7d 保留窗（与 RunRegistry PROBLEM_META_RETENTION
/// 对齐）内的 problem_key 行。created_at 为 sqlite datetime 串；解析失败
/// 的行跳过。未知 outcome fail-closed 到 Failed（与 repo 查询同策略）。
async fn load_problem_meta(pool: &SqlitePool) -> Vec<ProblemMetaRow> {
    let rows: Vec<(String, String, String, String, bool, Option<String>, String)> = match sqlx::query_as(
        "SELECT workspace_id, problem_key, id, outcome, verified, acked_at, created_at
         FROM agent_runs
         WHERE problem_key IS NOT NULL AND created_at > datetime('now', '-7 days')
         ORDER BY created_at ASC, rowid ASC",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!(error = %e, "agent snapshot: load problem meta failed");
            return vec![];
        }
    };
    rows.into_iter()
        .filter_map(
            |(workspace_id, problem_key, run_id, outcome, verified, acked_at, created_at)| {
                let occurred_at = chrono::NaiveDateTime::parse_from_str(&created_at, "%Y-%m-%d %H:%M:%S")
                    .map(|naive| chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc))
                    .map_err(|e| {
                        warn!(run_id = %run_id, error = %e, "agent snapshot: skip problem run with bad created_at");
                        e
                    })
                    .ok()?;
                Some(ProblemMetaRow {
                    workspace_id,
                    problem_key,
                    run_id,
                    outcome: Outcome::from_db(&outcome).unwrap_or(Outcome::Failed),
                    verified,
                    acked: acked_at.is_some(),
                    occurred_at,
                })
            },
        )
        .collect()
}

/// 启动顺序第 3 步：僵尸 run reconcile。DB 中 status='running' 的行必为
/// 上次进程崩溃遗留（restore 刚完成、尚无在飞 run）；registry 预热窗口
/// 认领的 run_id（已有完成报告）防御性排除。失败仅告警，不阻塞启动。
pub async fn reconcile_zombie_runs(db: &Db, runtime: &AgentRuntime) {
    let known_active: Vec<String> = runtime.active_runs().iter().map(|r| r.run_id.clone()).collect();
    let repo = AgentRunsRepository::new(db.pool().clone());
    match repo.interrupt_zombie_running_runs(&known_active).await {
        Ok(0) => {}
        Ok(n) => info!(marked = n, "startup zombie reconcile: running runs marked interrupted"),
        Err(e) => warn!(error = %e, "startup zombie reconcile failed"),
    }
}
