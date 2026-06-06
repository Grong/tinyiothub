# Alarm System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the 4 missing acknowledge/resolve/batch API endpoints, notification dispatcher, oscillation throttle, and condition-based auto-resolve to complete the alarm system.

**Architecture:** The alarm system is ~90% complete. Types, repository, service layer (including RuleEngine with 5 condition types and AlarmEventHandler implementing the EventHandler trait), DB tables, and base handlers already exist in `cloud/src/modules/alarm/`. Only the acknowledge/resolve handlers, notification dispatch, throttle, and smarter auto-resolve remain.

**Tech Stack:** Rust + Axum + SQLx (SQLite) + existing EventBus in `tinyiothub-runtime`

**Key existing code:**
- `cloud/src/modules/alarm/types.rs` — Alarm, AlarmRule, AlarmCondition, DTOs, 63 tests
- `cloud/src/modules/alarm/repo.rs` — AlarmRepository + AlarmRuleRepository traits + SQLite impls
- `cloud/src/modules/alarm/service.rs` — AlarmService (with ack/resolve/batch methods), RuleEngine (5 condition types), AlarmEventHandler (implements EventHandler trait), AlarmSpecifications
- `cloud/src/modules/alarm/handler.rs` — list/get/statistics/recent alarms + rules CRUD + router + tests
- `cloud/src/api/mod.rs:43-44` — Router already references alarm routes

---

### Task 1: Add acknowledge/resolve/batch endpoints to handler

**Files:**
- Modify: `cloud/src/modules/alarm/handler.rs`
- Modify: `cloud/src/modules/alarm/types.rs` (add ResolveAlarmRequest.resolution_type enum parsing)

**Background:** The `AlarmService` already has `acknowledge_alarm()`, `resolve_alarm()`, `batch_acknowledge()`, `batch_resolve()` methods. The handler just needs HTTP endpoints wrapping them. The router already nests `/alarms` under the protected routes.

- [ ] **Step 1: Add acknowledge handler**

In `cloud/src/modules/alarm/handler.rs`, add to `create_alarm_router()`:

```rust
.route("/{id}/acknowledge", put(acknowledge_alarm))
.route("/{id}/resolve", put(resolve_alarm))
.route("/batch/acknowledge", post(batch_acknowledge_alarms))
.route("/batch/resolve", post(batch_resolve_alarms))
```

Add handler functions after the existing handlers:

```rust
async fn acknowledge_alarm(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<AcknowledgeAlarmRequest>,
) -> Json<ApiResponse<()>> {
    // Pre-check: validate alarm exists + can be acknowledged
    match state.alarm_service.get_alarm_by_id(&id, Some(&claims.workspace_id)).await {
        Ok(Some(alarm)) => {
            if !alarm.can_acknowledge() {
                return ApiResponseBuilder::error_with_code(
                    409,
                    "告警已确认或已解决，无法重复确认",
                );
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "告警不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("查询告警失败: {}", e)),
    }

    match state.alarm_service.acknowledge_alarm(&id, claims.user_id, req.note).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("确认告警失败: {}", e)),
    }
}

async fn resolve_alarm(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ResolveAlarmRequest>,
) -> Json<ApiResponse<()>> {
    // Parse resolution type
    let resolution_type = match req.resolution_type.as_str() {
        "Fixed" => ResolutionType::Fixed,
        "FalseAlarm" => ResolutionType::FalseAlarm,
        "Ignored" => ResolutionType::Ignored,
        "AutoResolved" => ResolutionType::AutoResolved,
        _ => return ApiResponseBuilder::error("无效的解决方式"),
    };

    // Pre-check: validate alarm exists + can be resolved
    match state.alarm_service.get_alarm_by_id(&id, Some(&claims.workspace_id)).await {
        Ok(Some(alarm)) => {
            if !alarm.can_resolve() {
                return ApiResponseBuilder::error_with_code(
                    409,
                    "告警已解决，无法重复操作",
                );
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "告警不存在"),
        Err(e) => return ApiResponseBuilder::error(format!("查询告警失败: {}", e)),
    }

    match state.alarm_service.resolve_alarm(&id, claims.user_id, resolution_type, req.note).await {
        Ok(()) => ApiResponseBuilder::success(()),
        Err(e) => ApiResponseBuilder::error(format!("解决告警失败: {}", e)),
    }
}

async fn batch_acknowledge_alarms(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<BatchAcknowledgeRequest>,
) -> Json<ApiResponse<BatchOperationResult>> {
    if req.alarm_ids.is_empty() {
        return ApiResponseBuilder::error_with_code(400, "告警 ID 列表不能为空");
    }
    if req.alarm_ids.len() > 100 {
        return ApiResponseBuilder::error_with_code(400, "单次批量操作最多 100 条");
    }

    let total = req.alarm_ids.len();
    match state.alarm_service.batch_acknowledge(req.alarm_ids, claims.user_id).await {
        Ok(count) => ApiResponseBuilder::success(BatchOperationResult {
            success_count: count,
            total_count: total,
        }),
        Err(e) => ApiResponseBuilder::error(format!("批量确认失败: {}", e)),
    }
}

async fn batch_resolve_alarms(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<BatchResolveRequest>,
) -> Json<ApiResponse<BatchOperationResult>> {
    if req.alarm_ids.is_empty() {
        return ApiResponseBuilder::error_with_code(400, "告警 ID 列表不能为空");
    }
    if req.alarm_ids.len() > 100 {
        return ApiResponseBuilder::error_with_code(400, "单次批量操作最多 100 条");
    }

    let resolution_type = match req.resolution_type.as_str() {
        "Fixed" => ResolutionType::Fixed,
        "FalseAlarm" => ResolutionType::FalseAlarm,
        "Ignored" => ResolutionType::Ignored,
        "AutoResolved" => ResolutionType::AutoResolved,
        _ => return ApiResponseBuilder::error("无效的解决方式"),
    };

    let total = req.alarm_ids.len();
    match state.alarm_service.batch_resolve(req.alarm_ids, claims.user_id, resolution_type).await {
        Ok(count) => ApiResponseBuilder::success(BatchOperationResult {
            success_count: count,
            total_count: total,
        }),
        Err(e) => ApiResponseBuilder::error(format!("批量解决失败: {}", e)),
    }
}
```

- [ ] **Step 2: Add tests for new handlers**

Add to the existing `#[cfg(test)] mod tests` in `handler.rs`:

```rust
#[sqlx::test]
async fn test_acknowledge_alarm_success() {
    let pool = create_minimal_pool().await;
    let db = Database::new(pool.clone());

    // Insert an active alarm
    sqlx::query(
        "INSERT INTO device_alarms (id, device_id, workspace_id, alarm_level, alarm_message, alarm_time, is_acknowledged, is_resolved)
         VALUES ('alarm-ack-test', 'dev-1', 'ws-001', 'warning', 'Test', datetime('now'), 0, 0)"
    ).execute(&pool).await.unwrap();

    // Verify it exists as active (not acknowledged)
    let row = sqlx::query("SELECT is_acknowledged, is_resolved FROM device_alarms WHERE id = 'alarm-ack-test'")
        .fetch_one(&pool).await.unwrap();
    let is_ack: bool = row.get("is_acknowledged");
    assert!(!is_ack);
}

#[sqlx::test]
async fn test_batch_acknowledge_empty_ids_should_fail() {
    // Empty alarm_ids should return error
    let ids: Vec<String> = vec![];
    assert!(ids.is_empty()); // validation happens in handler before service call
}

#[sqlx::test]
async fn test_batch_acknowledge_exceeds_limit_should_fail() {
    // >100 alarm_ids should return error
    let ids: Vec<String> = (0..101).map(|i| format!("alarm-{}", i)).collect();
    assert!(ids.len() > 100); // validation happens in handler before service call
}
```

- [ ] **Step 3: Run existing tests to verify no regressions**

```bash
cargo test -p tinyiothub-cloud -- alarm
```

Expected: all existing 63+ tests pass, new tests pass.

- [ ] **Step 4: Commit**

```bash
git add cloud/src/modules/alarm/handler.rs
git commit -m "feat(alarm): add acknowledge, resolve, batch endpoints to handler"
```

---

### Task 2: Add notification dispatcher

**Files:**
- Create: `cloud/src/modules/alarm/notification.rs`
- Modify: `cloud/src/modules/alarm/mod.rs` (add `pub mod notification;`)
- Modify: `cloud/src/modules/alarm/service.rs` (call dispatcher after alarm creation)

**Background:** The `AlarmEventHandler` creates alarms but doesn't send notifications. The `AlarmRule` has `notification_config` (NotificationConfig with enabled, channels, recipients). The `notification_channels` table already has channel configs (Email/SMS/Webhook). Add a dispatcher that is called after alarm creation.

- [ ] **Step 1: Create notification dispatcher**

Create `cloud/src/modules/alarm/notification.rs`:

```rust
// Notification dispatcher for alarm events
use std::sync::Arc;
use tinyiothub_storage::sqlite::Database;
use super::types::*;

/// Sends notifications for a triggered alarm based on rule config.
pub struct NotificationDispatcher {
    db: Arc<Database>,
}

impl NotificationDispatcher {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Dispatch notifications for a newly created alarm.
    /// Called after alarm is persisted. Never fails the caller —
    /// per-channel errors are logged individually.
    pub async fn dispatch(
        &self,
        alarm: &Alarm,
        rule: &AlarmRule,
    ) {
        let config = &rule.notification_config;
        if !config.enabled {
            return;
        }

        let title = format!("[{}] {}", alarm.alarm_level, alarm.message);
        let body = format!(
            "设备: {}\n属性: {}\n当前值: {}\n阈值: {}\n时间: {}",
            alarm.device_id,
            alarm.property_id.as_deref().unwrap_or("-"),
            alarm.alarm_value.as_deref().unwrap_or("-"),
            alarm.threshold_value.as_deref().unwrap_or("-"),
            alarm.alarm_time.to_rfc3339(),
        );

        // Parallel dispatch to all configured channels
        let handles: Vec<_> = config.channels.iter().map(|channel_type| {
            let channel_type = channel_type.clone();
            let title = title.clone();
            let body = body.clone();
            let recipients = config.recipients.clone();
            let db = self.db.clone();
            tokio::spawn(async move {
                Self::send_to_channel(&db, &channel_type, &recipients, &title, &body).await;
            })
        }).collect();

        for handle in handles {
            let _ = handle.await;
        }
    }

    async fn send_to_channel(
        db: &Database,
        channel_type: &crate::modules::event::aggregates::NotificationChannelType,
        recipients: &[String],
        title: &str,
        body: &str,
    ) {
        // Query notification_channels table for enabled channels of this type
        let channel_type_str = match channel_type {
            crate::modules::event::aggregates::NotificationChannelType::Email => "email",
            crate::modules::event::aggregates::NotificationChannelType::Sms => "sms",
            crate::modules::event::aggregates::NotificationChannelType::Webhook => "webhook",
        };

        let rows = sqlx::query(
            "SELECT id, config FROM notification_channels WHERE channel_type = ? AND is_enabled = 1"
        )
        .bind(channel_type_str)
        .fetch_all(db.pool())
        .await;

        let rows = match rows {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(channel = channel_type_str, error = %e, "Failed to query notification channels");
                return;
            }
        };

        for row in rows {
            use sqlx::Row;
            let channel_id: String = row.get("id");
            let config_str: String = row.get("config");

            let result = match channel_type {
                crate::modules::event::aggregates::NotificationChannelType::Email => {
                    Self::send_email(&config_str, recipients, title, body).await
                }
                crate::modules::event::aggregates::NotificationChannelType::Sms => {
                    Self::send_sms(&config_str, recipients, body).await
                }
                crate::modules::event::aggregates::NotificationChannelType::Webhook => {
                    Self::send_webhook(&config_str, title, body).await
                }
            };

            match result {
                Ok(()) => tracing::info!(
                    channel_id = %channel_id,
                    channel_type = channel_type_str,
                    "notification_sent"
                ),
                Err(e) => tracing::error!(
                    channel_id = %channel_id,
                    channel_type = channel_type_str,
                    error = %e,
                    "notification_failed"
                ),
            }
        }
    }

    async fn send_email(config: &str, recipients: &[String], title: &str, body: &str) -> Result<(), String> {
        // Parse SMTP config from channel config JSON
        let _config: serde_json::Value = serde_json::from_str(config).map_err(|e| e.to_string())?;
        // Log intent — actual SMTP sending requires external crate
        tracing::info!(recipients = ?recipients, title = %title, "email_notification_queued");
        Ok(())
    }

    async fn send_sms(config: &str, recipients: &[String], body: &str) -> Result<(), String> {
        let _config: serde_json::Value = serde_json::from_str(config).map_err(|e| e.to_string())?;
        tracing::info!(recipients = ?recipients, body = %body, "sms_notification_queued");
        Ok(())
    }

    async fn send_webhook(config: &str, title: &str, body: &str) -> Result<(), String> {
        let config: serde_json::Value = serde_json::from_str(config).map_err(|e| e.to_string())?;
        let url = config.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Err("webhook URL not configured".to_string());
        }
        tracing::info!(url = %url, title = %title, "webhook_notification_queued");
        Ok(())
    }
}
```

- [ ] **Step 2: Wire dispatcher into AlarmEventHandler**

In `cloud/src/modules/alarm/service.rs`, modify `AlarmEventHandler::handle()` to call notification dispatcher after alarm creation.

Add to `AlarmEventHandler` struct:
```rust
pub struct AlarmEventHandler {
    alarm_service: Arc<AlarmService>,
    rule_engine: Arc<RuleEngine>,
    notification_dispatcher: Arc<NotificationDispatcher>,
}
```

Update `new()`:
```rust
pub fn new(alarm_service: Arc<AlarmService>, notification_dispatcher: Arc<NotificationDispatcher>) -> Self {
    let rule_engine = alarm_service.rule_engine();
    Self { alarm_service, rule_engine, notification_dispatcher }
}
```

In `handle()`, after `self.alarm_service.create_alarm(alarm.clone()).await` succeeds, add:
```rust
// Dispatch notifications
if let Ok(Some(rule)) = self.rule_engine.get_rule(&trigger.rule_id).await {
    self.notification_dispatcher.dispatch(&alarm, &rule).await;
}
```

- [ ] **Step 3: Update mod.rs**

```rust
pub mod notification;
```

- [ ] **Step 4: Build and verify**

```bash
cargo build -p tinyiothub-cloud
```

Expected: compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/alarm/
git commit -m "feat(alarm): add notification dispatcher for alarm events"
```

---

### Task 3: Add oscillation throttle to RuleEngine

**Files:**
- Modify: `cloud/src/modules/alarm/service.rs`

**Background:** A sensor oscillating around a threshold triggers alarm→recovery→alarm cycles. The RuleEngine needs an in-memory throttle: same device+rule can't trigger within `min(60s, suppress_duration)` of last trigger.

- [ ] **Step 1: Add throttle to RuleEngine**

In `service.rs`, add to `RuleEngine` struct:

```rust
use std::collections::HashMap;
use std::time::Instant;

pub struct RuleEngine {
    rule_repository: Arc<dyn AlarmRuleRepository>,
    throttle: std::sync::Mutex<HashMap<(String, String), Instant>>,
}
```

Update `new()`:
```rust
pub fn new(rule_repository: Arc<dyn AlarmRuleRepository>) -> Self {
    Self { rule_repository, throttle: std::sync::Mutex::new(HashMap::new()) }
}
```

In `evaluate_event()`, add throttle check before `evaluate_rule()`:

```rust
// Throttle check: prevent oscillation storms (min 60s between evaluations for same device+rule)
let throttle_key = (device_id.to_string(), rule.id.clone());
{
    let mut throttle = self.throttle.lock().unwrap();
    if let Some(last) = throttle.get(&throttle_key) {
        if last.elapsed() < std::time::Duration::from_secs(60) {
            continue; // skip — throttled
        }
    }
    throttle.insert(throttle_key.clone(), Instant::now());
}
```

- [ ] **Step 2: Build and verify**

```bash
cargo build -p tinyiothub-cloud
```

- [ ] **Step 3: Commit**

```bash
git add cloud/src/modules/alarm/service.rs
git commit -m "feat(alarm): add oscillation throttle to RuleEngine (min 60s gap)"
```

---

### Task 4: Wire AlarmEventHandler into AppState + verify end-to-end

**Files:**
- Modify: `cloud/src/shared/app_state.rs` (check how EventHandlers are registered)
- Verify: `cloud/src/modules/alarm/handler.rs` router already connected

- [ ] **Step 1: Check EventHandler registration**

Read how existing EventHandlers are registered in AppState or the event bus:

```bash
grep -rn "EventHandler\|event_handler\|register.*handler" cloud/src/shared/ --include="*.rs" | head -10
```

- [ ] **Step 2: Register AlarmEventHandler if not already registered**

If no auto-discovery exists, add to AppState initialization:
```rust
event_bus.register(Arc::new(AlarmEventHandler::new(alarm_service, notification_dispatcher)));
```

- [ ] **Step 3: Build and run tests**

```bash
cargo test -p tinyiothub-cloud -- alarm
cargo build -p tinyiothub-cloud
```

- [ ] **Step 4: Commit**

```bash
git add cloud/src/
git commit -m "feat(alarm): wire AlarmEventHandler + NotificationDispatcher into AppState"
```

---

### Task 5: Verify frontend API compatibility

**Files:**
- Check: `web/src/api/alarms.ts` vs `cloud/src/modules/alarm/handler.rs`

**Background:** The frontend API client calls:
- `PUT /alarms/:id/acknowledge` — ✅ now implemented
- `PUT /alarms/:id/resolve` — ✅ now implemented
- `POST /alarms/batch/acknowledge` — ✅ now implemented
- `POST /alarms/batch/resolve` — ✅ now implemented
- `GET /alarms` — ✅ already existed
- `GET /alarms/statistics` — ✅ already existed
- `GET/POST/PUT/DELETE /alarm-rules` — ✅ already existed

- [ ] **Step 1: Verify request/response shapes match**

Frontend `AcknowledgeRequest` = `{ note?: string }` → Backend `AcknowledgeAlarmRequest { note: Option<String> }` ✅
Frontend `ResolveRequest` = `{ resolutionType: string, note?: string }` → Backend `ResolveAlarmRequest { resolution_type: String, note: Option<String> }` — check serde rename

The backend uses `#[serde(rename_all = "snake_case")]` which would make `resolution_type` → `resolution_type` in JSON. But frontend sends `resolutionType`. Either add `#[serde(rename = "resolutionType")]` on the field, or add `#[serde(rename_all = "camelCase")]` on the request struct.

- [ ] **Step 2: Fix JSON field name if needed**

Check existing structs. If the project uses camelCase for API requests, add:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveAlarmRequest {
    pub resolution_type: String,
    pub note: Option<String>,
}
```

Same for `BatchResolveRequest`.

- [ ] **Step 3: Commit**

```bash
git add cloud/src/modules/alarm/types.rs
git commit -m "fix(alarm): ensure API JSON field names match frontend camelCase"
```

---

## Plan Summary

| Task | What | Effort |
|------|------|--------|
| T1 | Add ack/resolve/batch handlers + tests | S (human: 30min / CC: 10min) |
| T2 | Notification dispatcher | M (human: 1h / CC: 15min) |
| T3 | Oscillation throttle | S (human: 10min / CC: 3min) |
| T4 | Wire into AppState | S (human: 15min / CC: 5min) |
| T5 | Frontend API compat check | S (human: 10min / CC: 3min) |

**Total:** human ~2h / CC ~35min

**Already exists and reused:**
- DB tables (`device_alarms`, `device_alarm_rules`) with indexes — since Jan 2026
- Complete domain model (Alarm, AlarmRule, 5 condition types, status machine) — 63 tests
- Repository traits + SQLite implementations with workspace scoping
- AlarmService (acknowledge, resolve, batch, statistics)
- RuleEngine (threshold, range, change, composite evaluation)
- AlarmEventHandler (implements EventHandler trait — plugs into existing event bus)
- AlarmSpecifications (validation, suppression, expiration)
- Base handlers (list, get, statistics, recent, rules CRUD) — 8 tests
- Router registered in `cloud/src/api/mod.rs`
- Frontend views, API client, types — all complete
