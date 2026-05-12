# MQTT Gateway Pairing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edge gateways connect to TinyIoTHub via MQTT with zero-config pairing — gateway shows a 6-digit code, user enters it on the platform, gateway gets provisioned.

**Architecture:** Platform MQTT client listens on `tinyiothub/pairing/#` (anonymous) for gateway announce messages. User submits pairing code via REST API → platform creates Device in DB → publishes credentials via `pairing/{code}/response` → gateway reconnects with formal credentials. Sub-devices report through gateway topics, displayed flat in the device list via `linked_gateway` field.

**Tech Stack:** Rust (Axum + rumqttc + SQLx), Lit 3 + TypeScript + Vite, Eclipse Mosquitto, Docker

---

### Task 1: DB Migration — Add linked_gateway and fingerprint to devices

**Files:**
- Create: `cloud/migrations/20260511000001_add_gateway_fields_to_devices.sql`
- Modify: `crates/tinyiothub-core/src/models/device.rs`

- [ ] **Step 1: Write the migration SQL**

```sql
-- cloud/migrations/20260511000001_add_gateway_fields_to_devices.sql
ALTER TABLE devices ADD COLUMN linked_gateway TEXT;
ALTER TABLE devices ADD COLUMN fingerprint TEXT;

CREATE INDEX IF NOT EXISTS idx_devices_linked_gateway ON devices(linked_gateway);
CREATE INDEX IF NOT EXISTS idx_devices_fingerprint ON devices(fingerprint);
```

- [ ] **Step 2: Run migration to verify it applies**

Run: `cd cloud && sqlx migrate run --database-url sqlite:data/test.db`
Expected: Migration applied successfully, no errors.

- [ ] **Step 3: Add linked_gateway and fingerprint to the Device struct**

Add two fields to `crates/tinyiothub-core/src/models/device.rs` in the `Device` struct, after `parent_id`:

```rust
    pub parent_id: Option<String>,
    /// 子设备关联的网关 device_id（扁平展示用）
    pub linked_gateway: Option<String>,
    /// 网关硬件指纹（MAC 等）
    pub fingerprint: Option<String>,
    pub product_id: Option<String>,
```

Add same fields to `CreateDeviceRequest`:

```rust
    pub parent_id: Option<String>,
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
    pub product_id: Option<String>,
```

Add same fields to `UpdateDeviceRequest`:

```rust
    pub parent_id: Option<String>,
    pub linked_gateway: Option<String>,
    pub fingerprint: Option<String>,
    pub product_id: Option<String>,
```

- [ ] **Step 4: Run existing tests to verify no regressions**

Run: `cargo test -p tinyiothub-core`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add cloud/migrations/20260511000001_add_gateway_fields_to_devices.sql crates/tinyiothub-core/src/models/device.rs
git commit -m "feat(gateway): add linked_gateway and fingerprint fields to devices table"
```

---

### Task 2: Storage Layer — Update SQLite device queries for new fields

**Files:**
- Modify: `crates/tinyiothub-storage/src/sqlite/device.rs`
- Modify: `crates/tinyiothub-storage/src/sqlite/device_row_mapper.rs`

- [ ] **Step 1: Add linked_gateway filter to DeviceQueryParams**

Modify `crates/tinyiothub-storage/src/sqlite/device_row_mapper.rs` — if there's a row mapper that maps column names, add `linked_gateway` and `fingerprint` to the column list.

- [ ] **Step 2: Update INSERT and UPDATE SQL in device.rs**

In `crates/tinyiothub-storage/src/sqlite/device.rs`, find the INSERT statement and add `linked_gateway, fingerprint` columns:

```rust
// INSERT statement should include linked_gateway and fingerprint
"INSERT INTO devices (id, name, display_name, device_type, address, description, position,
 driver_name, device_model, protocol_type, factory_name, linked_data, driver_options,
 status, parent_id, linked_gateway, fingerprint, product_id, workspace_id, created_at, updated_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)"
```

- [ ] **Step 3: Add query_by_linked_gateway method**

Add a method to find devices by `linked_gateway`:

```rust
pub async fn find_by_linked_gateway(
    pool: &SqlitePool,
    gateway_id: &str,
) -> Result<Vec<Device>, StorageError> {
    let rows = sqlx::query_as::<_, DeviceRow>(
        "SELECT * FROM devices WHERE linked_gateway = ?1 ORDER BY created_at DESC"
    )
    .bind(gateway_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into()).collect())
}
```

- [ ] **Step 4: Run storage tests**

Run: `cargo test -p tinyiothub-storage`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/tinyiothub-storage/src/sqlite/device.rs crates/tinyiothub-storage/src/sqlite/device_row_mapper.rs
git commit -m "feat(storage): add linked_gateway query and new field support"
```

---

### Task 3: Gateway Module — Types

**Files:**
- Create: `cloud/src/modules/gateway/mod.rs`
- Create: `cloud/src/modules/gateway/types.rs`

- [ ] **Step 1: Write types.rs with pairing request/response and MQTT message types**

```rust
// cloud/src/modules/gateway/types.rs
use serde::{Deserialize, Serialize};

/// 配对请求（前端提交）
#[derive(Debug, Deserialize)]
pub struct PairingRequest {
    pub code: String,
    pub workspace_id: Option<String>,
}

/// 配对响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingResponse {
    pub device_id: String,
    pub device_name: String,
    pub hostname: String,
    pub ip: String,
}

/// 网关宣告（MQTT 消息，网关→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingAnnounce {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub code: String,
    pub fingerprint: String,
    pub hostname: String,
    pub os: String,
    pub ip: String,
    pub hw_model: String,
}

/// 配对响应（MQTT 消息，平台→网关）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingAck {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub success: bool,
    pub device_id: String,
    pub workspace_id: String,
    pub credentials: MqttCredentials,
    pub topics: GatewayTopics,
    pub keepalive: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MqttCredentials {
    pub client_id: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayTopics {
    pub status: String,
    pub telemetry: String,
    pub event: String,
    pub command: String,
    pub config: String,
    pub device_discover: String,
    pub device_telemetry: String,
}

/// 子设备发现消息（MQTT，网关→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDiscoverMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub devices: Vec<DiscoveredDevice>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredDevice {
    pub name: String,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
}

/// 遥测消息（MQTT，网关/子设备→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// 子设备遥测消息
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceTelemetryMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub device_id: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

/// 状态消息（MQTT，网关→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StatusMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub status: String,
    pub uptime: Option<u64>,
    pub timestamp: i64,
}

/// 指令下发请求（前端→平台）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandRequest {
    pub device_id: String,
    pub action: String,
    pub params: serde_json::Value,
}

/// 指令下发消息（MQTT，平台→网关）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub command_id: String,
    pub device_id: String,
    pub action: String,
    pub params: serde_json::Value,
    pub timestamp: i64,
}

/// 网关配置下发消息（MQTT，平台→网关）
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConfigMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub config: serde_json::Value,
    pub timestamp: i64,
}
```

- [ ] **Step 2: Write mod.rs to register the module**

```rust
// cloud/src/modules/gateway/mod.rs
pub mod types;
pub mod pairing;
pub mod service;
pub mod handler;
```

- [ ] **Step 3: Add `mod gateway` to cloud/src/modules/mod.rs**

After the existing module declarations, add:
```rust
pub mod gateway;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p tinyiothub-cloud`
Expected: No compile errors (unused import warnings are fine at this stage).

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/gateway/mod.rs cloud/src/modules/gateway/types.rs cloud/src/modules/mod.rs
git commit -m "feat(gateway): add gateway module types for pairing and MQTT messages"
```

---

### Task 4: PairingCache — In-memory pairing code store

**Files:**
- Create: `cloud/src/modules/gateway/pairing.rs`

- [ ] **Step 1: Write the failing test for PairingCache**

Create the test inline in `pairing.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_pairing_cache_insert_and_get() {
        let cache = PairingCache::new(100);
        let entry = PairingEntry {
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now(),
            attempts: HashMap::new(),
        };
        cache.insert("123456".into(), entry);
        assert!(cache.get("123456").is_some());
        assert!(cache.get("999999").is_none());
    }

    #[tokio::test]
    async fn test_pairing_cache_ttl_expiry() {
        let cache = PairingCache::new(100);
        let entry = PairingEntry {
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now() - Duration::from_secs(301),
            attempts: HashMap::new(),
        };
        cache.insert("123456".into(), entry);
        assert!(cache.get("123456").is_none()); // expired
    }

    #[tokio::test]
    async fn test_pairing_cache_rate_limit_per_user() {
        let cache = PairingCache::new(100);
        let mut entry = PairingEntry {
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now(),
            attempts: HashMap::new(),
        };
        cache.insert("123456".into(), entry);

        // Allow 5 attempts
        for _ in 0..5 {
            assert!(cache.check_and_increment_attempts("123456", "user1"));
        }
        // 6th should fail
        assert!(!cache.check_and_increment_attempts("123456", "user1"));
    }

    #[tokio::test]
    async fn test_pairing_cache_remove() {
        let cache = PairingCache::new(100);
        let entry = PairingEntry {
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now(),
            attempts: HashMap::new(),
        };
        cache.insert("123456".into(), entry);
        cache.remove("123456");
        assert!(cache.get("123456").is_none());
    }

    #[tokio::test]
    async fn test_pairing_cache_is_full() {
        let cache = PairingCache::new(2);
        let make_entry = || PairingEntry {
            fingerprint: "aa:bb:cc".into(),
            hostname: "gw-01".into(),
            os: "Linux".into(),
            ip: "192.168.1.1".into(),
            hw_model: "RPi5".into(),
            created_at: Instant::now(),
            attempts: HashMap::new(),
        };
        cache.insert("111111".into(), make_entry());
        cache.insert("222222".into(), make_entry());
        assert!(cache.is_full());
        // Inserting a 3rd should be rejected
        assert!(!cache.try_insert("333333".into(), make_entry()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tinyiothub-cloud -- pairing_cache`
Expected: FAIL — PairingCache not defined yet.

- [ ] **Step 3: Write the PairingCache implementation**

```rust
// cloud/src/modules/gateway/pairing.rs
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const PAIRING_CODE_TTL: Duration = Duration::from_secs(300); // 5 minutes
const MAX_ATTEMPTS_PER_USER: u32 = 5;
const DEFAULT_MAX_ENTRIES: usize = 10000;

pub struct PairingCache {
    entries: Arc<RwLock<HashMap<String, PairingEntry>>>,
    max_entries: usize,
}

#[derive(Debug, Clone)]
pub struct PairingEntry {
    pub fingerprint: String,
    pub hostname: String,
    pub os: String,
    pub ip: String,
    pub hw_model: String,
    pub created_at: Instant,
    pub attempts: HashMap<String, u32>,
}

impl PairingCache {
    pub fn new(max_entries: usize) -> Self {
        let cache = Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        };
        cache.spawn_cleanup_task();
        cache
    }

    pub async fn get(&self, code: &str) -> Option<PairingEntry> {
        let entries = self.entries.read().await;
        entries.get(code).and_then(|e| {
            if e.created_at.elapsed() > PAIRING_CODE_TTL {
                None
            } else {
                Some(e.clone())
            }
        })
    }

    pub async fn try_insert(&self, code: String, entry: PairingEntry) -> bool {
        let mut entries = self.entries.write().await;
        if entries.len() >= self.max_entries {
            return false;
        }
        entries.insert(code, entry);
        true
    }

    pub async fn insert(&self, code: String, entry: PairingEntry) {
        let mut entries = self.entries.write().await;
        entries.insert(code, entry);
    }

    pub async fn remove(&self, code: &str) {
        let mut entries = self.entries.write().await;
        entries.remove(code);
    }

    pub async fn check_and_increment_attempts(&self, code: &str, user_id: &str) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(code) {
            if entry.created_at.elapsed() > PAIRING_CODE_TTL {
                return false;
            }
            let count = entry.attempts.entry(user_id.to_string()).or_insert(0);
            if *count >= MAX_ATTEMPTS_PER_USER {
                return false;
            }
            *count += 1;
            true
        } else {
            false
        }
    }

    pub async fn is_full(&self) -> bool {
        let entries = self.entries.read().await;
        entries.len() >= self.max_entries
    }

    fn spawn_cleanup_task(&self) {
        let entries = Arc::clone(&self.entries);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut map = entries.write().await;
                map.retain(|_, entry| entry.created_at.elapsed() < PAIRING_CODE_TTL);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    // tests from Step 1 go here
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tinyiothub-cloud -- pairing_cache`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/gateway/pairing.rs
git commit -m "feat(gateway): add PairingCache with TTL, rate limiting, and capacity guard"
```

---

### Task 5: Gateway Service — Business logic

**Files:**
- Create: `cloud/src/modules/gateway/service.rs`

- [ ] **Step 1: Write the service scaffolding and first test**

Write `service.rs` with the `GatewayService` struct. The service depends on:
- `PairingCache` (from `pairing.rs`)
- `SqlitePool` (for device creation)
- MQTT client sender (for publishing credentials)

```rust
// cloud/src/modules/gateway/service.rs
use crate::modules::gateway::types::*;
use crate::modules::gateway::pairing::PairingCache;
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use uuid::Uuid;

/// MQTT 发布指令（通过 channel 发送到 MQTT client）
pub enum MqttPublish {
    PairingAck {
        code: String,
        ack: PairingAck,
    },
}

pub struct GatewayService {
    pool: SqlitePool,
    cache: PairingCache,
    mqtt_tx: mpsc::Sender<MqttPublish>,
}

impl GatewayService {
    pub fn new(pool: SqlitePool, cache: PairingCache, mqtt_tx: mpsc::Sender<MqttPublish>) -> Self {
        Self { pool, cache, mqtt_tx }
    }

    /// 用户提交配对码，执行配对校验
    pub async fn pair_device(
        &self,
        user_id: &str,
        req: PairingRequest,
    ) -> Result<PairingResponse, PairingError> {
        let code = req.code.trim().to_string();
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(PairingError::InvalidCode);
        }

        // 1. 检查缓存是否满
        if self.cache.is_full().await {
            return Err(PairingError::ServiceBusy);
        }

        // 2. 检查宣告是否存在
        let announce = self.cache.get(&code).await
            .ok_or(PairingError::CodeNotFound)?;

        // 3. 检查尝试次数
        if !self.cache.check_and_increment_attempts(&code, user_id).await {
            return Err(PairingError::TooManyAttempts);
        }

        // 4. 创建 Device
        let device_id = Uuid::new_v4().to_string();
        let device_name = announce.hostname.clone();
        let workspace_id = req.workspace_id.clone().unwrap_or_default();
        let password = generate_device_password();

        sqlx::query(
            "INSERT INTO devices (id, name, device_type, protocol_type, fingerprint, linked_gateway, status, workspace_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        )
        .bind(&device_id)
        .bind(&device_name)
        .bind("gateway")
        .bind("mqtt")
        .bind(&announce.fingerprint)
        .bind::<Option<String>>(None)
        .bind(1i32) // online
        .bind(&workspace_id)
        .bind(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
        .bind(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(?e, "Failed to create device during pairing");
            PairingError::Internal
        })?;

        // 5. 发布 MQTT 配对响应
        let ack = PairingAck {
            msg_type: "pairing_ack".into(),
            success: true,
            device_id: device_id.clone(),
            workspace_id: workspace_id.clone(),
            credentials: MqttCredentials {
                client_id: device_id.clone(),
                username: device_id.clone(),
                password: password.clone(),
            },
            topics: GatewayTopics {
                status: format!("tinyiothub/{}/gateway/{}/status", workspace_id, device_id),
                telemetry: format!("tinyiothub/{}/gateway/{}/telemetry", workspace_id, device_id),
                event: format!("tinyiothub/{}/gateway/{}/event", workspace_id, device_id),
                command: format!("tinyiothub/{}/gateway/{}/command", workspace_id, device_id),
                config: format!("tinyiothub/{}/gateway/{}/config", workspace_id, device_id),
                device_discover: format!("tinyiothub/{}/gateway/{}/device/discover", workspace_id, device_id),
                device_telemetry: format!("tinyiothub/{}/gateway/{}/device/+/telemetry", workspace_id, device_id),
            },
            keepalive: 60,
        };

        if self.mqtt_tx.send(MqttPublish::PairingAck { code: code.clone(), ack }).await.is_err() {
            // 回滚：删除刚创建的 Device
            let _ = sqlx::query("DELETE FROM devices WHERE id = ?1")
                .bind(&device_id)
                .execute(&self.pool)
                .await;
            tracing::error!("MQTT channel closed, rolled back device creation");
            return Err(PairingError::MqttPublishFailed);
        }

        // 6. 从缓存中移除配对码（一次性）
        self.cache.remove(&code).await;

        tracing::info!(
            code = %code,
            device_id = %device_id,
            fingerprint = %announce.fingerprint,
            "Pairing successful"
        );

        Ok(PairingResponse {
            device_id,
            device_name,
            hostname: announce.hostname,
            ip: announce.ip,
        })
    }

    /// 处理网关宣告
    pub async fn handle_announce(&self, announce: PairingAnnounce) -> Result<(), AnnounceError> {
        let entry = PairingEntry {
            fingerprint: announce.fingerprint.clone(),
            hostname: announce.hostname.clone(),
            os: announce.os.clone(),
            ip: announce.ip.clone(),
            hw_model: announce.hw_model.clone(),
            created_at: std::time::Instant::now(),
            attempts: std::collections::HashMap::new(),
        };

        let inserted = self.cache.try_insert(announce.code.clone(), entry).await;
        if !inserted {
            tracing::warn!(
                code = %announce.code,
                fingerprint = %announce.fingerprint,
                "Pairing cache full, rejecting announce"
            );
            return Err(AnnounceError::CacheFull);
        }

        tracing::debug!(
            code = %announce.code,
            fingerprint = %announce.fingerprint,
            hostname = %announce.hostname,
            "Received pairing announce"
        );
        Ok(())
    }

    /// 处理子设备发现
    pub async fn handle_device_discover(
        &self,
        gateway_id: &str,
        workspace_id: &str,
        msg: DeviceDiscoverMessage,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for device in &msg.devices {
            let sub_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO devices (id, name, device_type, protocol_type, address, driver_name, driver_options, linked_gateway, parent_id, status, workspace_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
            )
            .bind(&sub_id)
            .bind(&device.name)
            .bind(device.device_type.as_deref().unwrap_or("sensor"))
            .bind(device.protocol_type.as_deref())
            .bind(device.address.as_deref())
            .bind(device.driver_name.as_deref())
            .bind(device.driver_options.as_deref())
            .bind(gateway_id)
            .bind(gateway_id)
            .bind(1i32) // online — sub-device starts online when gateway is online
            .bind(workspace_id)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}

fn generate_device_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32).map(|_| rng.sample(rand::distributions::Alphanumeric) as char).collect()
}

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("未发现设备，请确认配对码是否正确")]
    CodeNotFound,
    #[error("配对码格式无效")]
    InvalidCode,
    #[error("尝试次数过多，请1分钟后重试")]
    TooManyAttempts,
    #[error("服务繁忙，请稍后重试")]
    ServiceBusy,
    #[error("配对暂时失败，请稍后重试")]
    Internal,
    #[error("MQTT发布失败，配对已回滚")]
    MqttPublishFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum AnnounceError {
    #[error("Pairing cache is full")]
    CacheFull,
}

use crate::modules::gateway::pairing::PairingEntry;
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p tinyiothub-cloud`
Expected: No errors from gateway module.

- [ ] **Step 3: Commit**

```bash
git add cloud/src/modules/gateway/service.rs
git commit -m "feat(gateway): add GatewayService with pairing, announce, and device discovery logic"
```

---

### Task 6: Gateway Handler — HTTP API

**Files:**
- Create: `cloud/src/modules/gateway/handler/mod.rs`
- Create: `cloud/src/modules/gateway/handler/pairing.rs`

- [ ] **Step 1: Write the pairing handler**

```rust
// cloud/src/modules/gateway/handler/pairing.rs
use axum::{extract::State, Json};
use crate::modules::gateway::service::GatewayService;
use crate::modules::gateway::types::PairingRequest;
use tinyiothub_web::response::ApiResponseBuilder;
use std::sync::Arc;

pub async fn pair_device(
    State(service): State<Arc<GatewayService>>,
    Json(req): Json<PairingRequest>,
) -> Json<serde_json::Value> {
    // user_id from auth extension — for now use "anonymous" (will be wired to JWT extractor)
    let user_id = "anonymous";

    match service.pair_device(user_id, req).await {
        Ok(response) => ApiResponseBuilder::success(serde_json::to_value(response).unwrap()),
        Err(e) => {
            let (code, msg) = match &e {
                crate::modules::gateway::service::PairingError::CodeNotFound => (404, e.to_string()),
                crate::modules::gateway::service::PairingError::InvalidCode => (400, e.to_string()),
                crate::modules::gateway::service::PairingError::TooManyAttempts => (429, e.to_string()),
                crate::modules::gateway::service::PairingError::ServiceBusy => (503, e.to_string()),
                _ => (500, "配对暂时失败，请稍后重试".to_string()),
            };
            ApiResponseBuilder::error(code, &msg)
        }
    }
}
```

- [ ] **Step 2: Write the handler module file**

```rust
// cloud/src/modules/gateway/handler/mod.rs
pub mod pairing;
```

- [ ] **Step 3: Register the route in cloud/src/server.rs**

Find the route registration section and add:

```rust
.route("/api/v1/gateway/pair", post(gateway::handler::pairing::pair_device))
```

Note: This step requires finding the exact route registration pattern in the existing server.rs.

- [ ] **Step 4: Verify compilation and route registration**

Run: `cargo check -p tinyiothub-cloud`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/gateway/handler/mod.rs cloud/src/modules/gateway/handler/pairing.rs
git commit -m "feat(gateway): add POST /api/v1/gateway/pair handler"
```

---

### Task 7: Platform MQTT Client

**Files:**
- Create: `cloud/src/shared/mqtt_client.rs`
- Modify: `cloud/Cargo.toml` (add rumqttc dep if not already)

- [ ] **Step 1: Check rumqttc is available as workspace dependency**

Run: `grep -A 2 'rumqttc\|mqtt' Cargo.toml`
Expected: Should find rumqttc in workspace dependencies (already used by the project).

- [ ] **Step 2: Write the platform MQTT client**

```rust
// cloud/src/shared/mqtt_client.rs
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::modules::gateway::service::MqttPublish;
use crate::modules::gateway::types::{
    DeviceDiscoverMessage, DeviceTelemetryMessage, PairingAnnounce, StatusMessage, TelemetryMessage,
};

pub struct PlatformMqttClient {
    client: AsyncClient,
    announce_tx: mpsc::Sender<PairingAnnounce>,
    mqtt_rx: mpsc::Receiver<MqttPublish>,
}

impl PlatformMqttClient {
    pub fn new(
        broker_url: &str,
        username: &str,
        password: &str,
        announce_tx: mpsc::Sender<PairingAnnounce>,
        mqtt_rx: mpsc::Receiver<MqttPublish>,
    ) -> Self {
        let client_id = format!("tinyiothub-platform-{}", uuid::Uuid::new_v4());
        let mut options = MqttOptions::new(&client_id, broker_url, 1883);
        options.set_credentials(username, password);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_max_packet_size(256 * 1024, 256 * 1024);

        let (client, eventloop) = AsyncClient::new(options, 100);

        let client_clone = client.clone();
        let announce_tx_clone = announce_tx.clone();

        // Spawn event handling task
        tokio::spawn(async move {
            Self::event_loop(eventloop, client_clone, announce_tx_clone).await;
        });

        Self { client, announce_tx, mqtt_rx }
    }

    async fn event_loop(
        mut eventloop: rumqttc::EventLoop,
        client: AsyncClient,
        announce_tx: mpsc::Sender<PairingAnnounce>,
    ) {
        // Subscribe to pairing announce topic
        client.subscribe("tinyiothub/pairing/announce", QoS::AtLeastOnce).await.ok();

        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    tracing::info!("Platform MQTT client connected");
                    // Re-subscribe after reconnect
                    client.subscribe("tinyiothub/pairing/announce", QoS::AtLeastOnce).await.ok();
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    let topic = publish.topic.clone();
                    if topic == "tinyiothub/pairing/announce" {
                        if let Ok(announce) = serde_json::from_slice::<PairingAnnounce>(&publish.payload) {
                            let _ = announce_tx.send(announce).await;
                        }
                    }
                    // Other topics handled as we add subscriptions
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(?e, "Platform MQTT event loop error");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Subscribe to gateway topics for a newly paired gateway
    pub async fn subscribe_gateway(&self, workspace_id: &str, device_id: &str) {
        let status_topic = format!("tinyiothub/{}/gateway/{}/status", workspace_id, device_id);
        let telemetry_topic = format!("tinyiothub/{}/gateway/{}/telemetry", workspace_id, device_id);
        let event_topic = format!("tinyiothub/{}/gateway/{}/event", workspace_id, device_id);
        let discover_topic = format!("tinyiothub/{}/gateway/{}/device/discover", workspace_id, device_id);
        let device_telemetry_topic = format!("tinyiothub/{}/gateway/{}/device/+/telemetry", workspace_id, device_id);

        self.client.subscribe(&status_topic, QoS::AtMostOnce).await.ok();
        self.client.subscribe(&telemetry_topic, QoS::AtMostOnce).await.ok();
        self.client.subscribe(&event_topic, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(&discover_topic, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(&device_telemetry_topic, QoS::AtMostOnce).await.ok();
    }

    /// Publish a pairing ack to the gateway
    pub async fn publish_pairing_ack(&self, code: &str, payload: &[u8]) {
        let topic = format!("tinyiothub/pairing/{}/response", code);
        self.client.publish(&topic, QoS::AtLeastOnce, false, payload).await.ok();
    }

    /// Publish a command to a gateway or sub-device
    pub async fn publish_command(&self, topic: &str, payload: &[u8]) {
        self.client.publish(topic, QoS::AtLeastOnce, false, payload).await.ok();
    }
}
```

- [ ] **Step 3: Update cloud/src/shared/mod.rs to include mqtt_client**

Add:
```rust
pub mod mqtt_client;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p tinyiothub-cloud`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add cloud/src/shared/mqtt_client.rs cloud/src/shared/mod.rs
git commit -m "feat(mqtt): add platform MQTT client with pairing announce subscription"
```

---

### Task 8: Device Service — Cascade delete and sub-device status

**Files:**
- Modify: `cloud/src/modules/device/service.rs`

- [ ] **Step 1: Add cascade delete to the existing device delete method**

Find the `delete_device` method in `cloud/src/modules/device/service.rs` and add sub-device deletion before the gateway deletion:

```rust
pub async fn delete_device(&self, device_id: &str) -> Result<(), ServiceError> {
    // Cascade: delete all sub-devices linked to this gateway
    sqlx::query("DELETE FROM devices WHERE linked_gateway = ?1")
        .bind(device_id)
        .execute(&self.pool)
        .await?;

    // Delete the gateway itself
    sqlx::query("DELETE FROM devices WHERE id = ?1")
        .bind(device_id)
        .execute(&self.pool)
        .await?;

    Ok(())
}
```

- [ ] **Step 2: Add sub-device status sync logic**

When a gateway goes offline, mark all its sub-devices as offline:

```rust
pub async fn sync_sub_device_status(
    pool: &SqlitePool,
    gateway_id: &str,
    status: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE devices SET status = ?1, updated_at = ?2 WHERE linked_gateway = ?3")
        .bind(status)
        .bind(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
        .bind(gateway_id)
        .execute(pool)
        .await?;
    Ok(())
}
```

- [ ] **Step 3: Run device tests**

Run: `cargo test -p tinyiothub-cloud -- device`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add cloud/src/modules/device/service.rs
git commit -m "feat(device): add cascade delete for gateway sub-devices and status sync"
```

---

### Task 9: Wire Everything Together — App State & Server Startup

**Files:**
- Modify: `cloud/src/server.rs` (or wherever AppState is constructed)
- Modify: `cloud/src/shared/app_state.rs` (if exists)

- [ ] **Step 1: Add GatewayService and MQTT channels to AppState**

Add to the AppState struct:
```rust
pub gateway_service: Arc<GatewayService>,
pub mqtt_client: Arc<PlatformMqttClient>,
```

- [ ] **Step 2: Initialize MQTT client and GatewayService on startup**

In the server startup function, after DB pool creation:

```rust
// Create channels
let (announce_tx, mut announce_rx) = mpsc::channel::<PairingAnnounce>(1000);
let (mqtt_tx, mqtt_rx) = mpsc::channel::<MqttPublish>(100);

// Create PairingCache
let pairing_cache = PairingCache::new(10000);

// Create GatewayService
let gateway_service = Arc::new(GatewayService::new(
    pool.clone(),
    pairing_cache,
    mqtt_tx,
));

// MQTT Client
let mqtt_broker = std::env::var("MQTT_BROKER_URL").unwrap_or_else(|_| "mqtt.tinyiothub.com".into());
let mqtt_username = std::env::var("MQTT_USERNAME").unwrap_or_else(|_| "admin".into());
let mqtt_password = std::env::var("MQTT_PASSWORD").unwrap_or_else(|_| String::new());
let mqtt_client = Arc::new(PlatformMqttClient::new(
    &mqtt_broker,
    &mqtt_username,
    &mqtt_password,
    announce_tx,
    mqtt_rx,
));

// Spawn announce handler
let gateway_service_clone = gateway_service.clone();
let mqtt_client_clone = mqtt_client.clone();
tokio::spawn(async move {
    while let Some(announce) = announce_rx.recv().await {
        if let Err(e) = gateway_service_clone.handle_announce(announce).await {
            tracing::warn!(?e, "Failed to handle pairing announce");
        }
    }
});

// Spawn MQTT publish handler
tokio::spawn(async move {
    // mqtt_rx is consumed by PlatformMqttClient — handle publish requests
    // In the PlatformMqttClient event loop, check for outgoing publishes
});
```

- [ ] **Step 3: Verify full compilation**

Run: `cargo check -p tinyiothub-cloud`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add cloud/src/server.rs cloud/src/shared/app_state.rs
git commit -m "feat(gateway): wire GatewayService and MQTT client into app startup"
```

---

### Task 10: Frontend — API Client

**Files:**
- Create: `web/src/api/gateway.ts`
- Modify: `web/src/api/devices.ts`

- [ ] **Step 1: Write the gateway API client**

```typescript
// web/src/api/gateway.ts
import { apiClient } from './client';

export interface PairingRequest {
  code: string;
  workspace_id?: string;
}

export interface PairingResponse {
  device_id: string;
  device_name: string;
  hostname: string;
  ip: string;
}

export async function pairGateway(req: PairingRequest): Promise<PairingResponse> {
  const res = await apiClient.post('/api/v1/gateway/pair', req);
  if (res.code !== 0) {
    throw new Error(res.msg || 'Pairing failed');
  }
  return res.result as PairingResponse;
}
```

- [ ] **Step 2: Verify frontend compiles**

Run: `cd web && npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/api/gateway.ts
git commit -m "feat(frontend): add gateway pairing API client"
```

---

### Task 11: Frontend — Pairing UI

**Files:**
- Create: `web/src/ui/views/gateway-pairing.ts`
- Modify: `web/src/ui/views/devices.ts` (add "Add Gateway" button)
- Modify: `web/src/styles/views/devices.css` (add gateway pairing styles)

- [ ] **Step 1: Write the gateway pairing modal component**

```typescript
// web/src/ui/views/gateway-pairing.ts
import { LitElement, html, css } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { pairGateway } from '../../api/gateway';

@customElement('gateway-pairing-dialog')
export class GatewayPairingDialog extends LitElement {
  static styles = css`
    :host {
      display: block;
    }
    .dialog-overlay {
      position: fixed;
      inset: 0;
      background: rgba(0,0,0,0.5);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 1000;
    }
    .dialog {
      background: white;
      border-radius: 12px;
      padding: 32px;
      width: 400px;
      max-width: 90vw;
    }
    .dialog h2 {
      margin: 0 0 8px;
      font-size: 20px;
    }
    .dialog p {
      color: #666;
      margin: 0 0 24px;
    }
    .code-input {
      display: flex;
      gap: 8px;
      justify-content: center;
      margin-bottom: 24px;
    }
    .code-input input {
      width: 48px;
      height: 56px;
      text-align: center;
      font-size: 24px;
      border: 2px solid #ddd;
      border-radius: 8px;
    }
    .code-input input:focus {
      border-color: #4f46e5;
      outline: none;
    }
    .actions {
      display: flex;
      gap: 12px;
      justify-content: flex-end;
    }
    .btn {
      padding: 8px 20px;
      border-radius: 8px;
      border: none;
      cursor: pointer;
      font-size: 14px;
    }
    .btn-primary {
      background: #4f46e5;
      color: white;
    }
    .btn-primary:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    .btn-cancel {
      background: #f3f4f6;
      color: #374151;
    }
    .error {
      color: #dc2626;
      font-size: 13px;
      margin-bottom: 16px;
    }
    .success {
      color: #16a34a;
      font-size: 14px;
      margin-bottom: 16px;
    }
  `;

  @state() private code = '';
  @state() private loading = false;
  @state() private error = '';
  @state() private success = false;

  render() {
    return html`
      <div class="dialog-overlay" @click=${this.handleOverlayClick}>
        <div class="dialog" @click=${(e: Event) => e.stopPropagation()}>
          <h2>Add Gateway Device</h2>
          <p>Enter the 6-digit code shown on your gateway screen.</p>

          ${this.error ? html`<div class="error">${this.error}</div>` : ''}
          ${this.success ? html`<div class="success">Gateway paired successfully! Refreshing device list...</div>` : ''}

          <div class="code-input">
            <input
              type="text"
              maxlength="6"
              .value=${this.code}
              @input=${this.handleCodeInput}
              @keydown=${this.handleKeyDown}
              placeholder="000000"
              ?disabled=${this.loading || this.success}
            />
          </div>

          <div class="actions">
            <button class="btn btn-cancel" @click=${this.close} ?disabled=${this.loading}>Cancel</button>
            <button
              class="btn btn-primary"
              @click=${this.pair}
              ?disabled=${this.loading || this.success || this.code.length !== 6}
            >
              ${this.loading ? 'Pairing...' : 'Pair'}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private handleCodeInput(e: InputEvent) {
    const input = e.target as HTMLInputElement;
    this.code = input.value.replace(/\D/g, '').slice(0, 6);
    this.error = '';
  }

  private handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && this.code.length === 6 && !this.loading) {
      this.pair();
    }
  }

  private async pair() {
    if (this.code.length !== 6) return;
    this.loading = true;
    this.error = '';
    try {
      await pairGateway({ code: this.code });
      this.success = true;
      setTimeout(() => {
        this.close();
        window.location.reload();
      }, 1500);
    } catch (e: any) {
      this.error = e.message || 'Pairing failed';
    } finally {
      this.loading = false;
    }
  }

  private handleOverlayClick() {
    if (!this.loading) this.close();
  }

  private close() {
    this.dispatchEvent(new CustomEvent('close'));
  }
}
```

- [ ] **Step 2: Add "Add Gateway" button to the devices view**

Open `web/src/ui/views/devices.ts` and add an "Add Gateway" button in the toolbar section:

```typescript
// Add import
import './gateway-pairing';

// In the component, add a state and render
@state() private showPairingDialog = false;

// In the render method, add a button and conditional dialog:
render() {
  return html`
    <!-- existing content -->
    <button @click=${() => this.showPairingDialog = true}>Add Gateway</button>
    ${this.showPairingDialog ? html`
      <gateway-pairing-dialog @close=${() => this.showPairingDialog = false}></gateway-pairing-dialog>
    ` : ''}
  `;
}
```

- [ ] **Step 3: Add "via gateway" label in device list**

In the device list rendering, when a device has `linked_gateway`, show a "via {gateway_name}" label:

```typescript
${device.linked_gateway ? html`<span class="via-gateway">via ${device.linked_gateway_name || device.linked_gateway}</span>` : ''}
```

- [ ] **Step 4: Verify frontend builds**

Run: `cd web && npm run build`
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views/gateway-pairing.ts web/src/ui/views/devices.ts
git commit -m "feat(frontend): add gateway pairing dialog and 'via gateway' label in device list"
```

---

### Task 12: Edge Gateway — Cargo Setup and Config

**Files:**
- Modify: `edge/Cargo.toml`
- Create: `edge/src/config.rs`

- [ ] **Step 1: Update edge/Cargo.toml with new dependencies**

```toml
[dependencies]
tinyiothub-core = { workspace = true }
tinyiothub-runtime = { workspace = true }
tinyiothub-plugin = { workspace = true }
tokio = { workspace = true }
rumqttc = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
rand = "0.8"
uuid = { version = "1", features = ["v4"] }
chrono = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Write config.rs**

```rust
// edge/src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeConfig {
    pub mqtt_broker: String,
    pub mqtt_port: u16,
    pub pairing_interval_secs: u64,
    pub heartbeat_interval_secs: u64,
    pub credentials_file: PathBuf,
    pub discovery_enabled: bool,
    pub discovery_interval_secs: u64,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            mqtt_broker: "mqtt.tinyiothub.com".into(),
            mqtt_port: 1883,
            pairing_interval_secs: 30,
            heartbeat_interval_secs: 30,
            credentials_file: PathBuf::from("/app/data/credentials.json"),
            discovery_enabled: true,
            discovery_interval_secs: 300,
        }
    }
}

impl EdgeConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(broker) = std::env::var("EDGE_MQTT_BROKER") {
            config.mqtt_broker = broker;
        }
        if let Ok(port) = std::env::var("EDGE_MQTT_PORT") {
            config.mqtt_port = port.parse().unwrap_or(1883);
        }
        if let Ok(path) = std::env::var("EDGE_CREDENTIALS_FILE") {
            config.credentials_file = PathBuf::from(path);
        }
        config
    }
}

/// Persisted credentials after successful pairing
#[derive(Debug, Serialize, Deserialize)]
pub struct GatewayCredentials {
    pub device_id: String,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub workspace_id: String,
}

impl GatewayCredentials {
    pub fn load(path: &PathBuf) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p tinyiothub-edge`
Expected: No errors (unused imports are OK).

- [ ] **Step 4: Commit**

```bash
git add edge/Cargo.toml edge/src/config.rs
git commit -m "feat(edge): add config loading and credential persistence"
```

---

### Task 13: Edge Gateway — Pairing Code Generation

**Files:**
- Create: `edge/src/pairing.rs`

- [ ] **Step 1: Write pairing code generator**

```rust
// edge/src/pairing.rs
use rand::Rng;
use std::time::{Duration, Instant};

const PAIRING_CODE_REFRESH_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

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

    /// Display format: "482 916"
    pub fn display_format(code: &str) -> String {
        format!("{} {}", &code[..3], &code[3..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_is_six_digits() {
        let mut gen = PairingCodeGenerator::new();
        let code = gen.get_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_same_code_within_ttl() {
        let mut gen = PairingCodeGenerator::new();
        let code1 = gen.get_code().to_string();
        let code2 = gen.get_code().to_string();
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_display_format() {
        assert_eq!(PairingCodeGenerator::display_format("482916"), "482 916");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p tinyiothub-edge -- pairing`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add edge/src/pairing.rs
git commit -m "feat(edge): add pairing code generator with 5-min TTL"
```

---

### Task 14: Edge Gateway — MQTT Client

**Files:**
- Modify: `edge/src/mqtt_client.rs` (replace placeholder comment with full implementation)

- [ ] **Step 1: Write the edge MQTT client**

```rust
// edge/src/mqtt_client.rs
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::{EdgeConfig, GatewayCredentials};

pub struct EdgeMqttClient {
    client: AsyncClient,
}

pub enum MqttEvent {
    PairingAck(serde_json::Value),
    Command(serde_json::Value),
    Config(serde_json::Value),
}

impl EdgeMqttClient {
    /// Create an anonymous client for pairing
    pub fn new_anonymous(config: &EdgeConfig, event_tx: mpsc::Sender<MqttEvent>) -> Self {
        let client_id = format!("edge-pairing-{}", uuid::Uuid::new_v4());
        let mut options = MqttOptions::new(&client_id, &config.mqtt_broker, config.mqtt_port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_clean_session(true);

        let (client, eventloop) = AsyncClient::new(options, 100);
        let client_clone = client.clone();

        tokio::spawn(async move {
            Self::anonymous_event_loop(eventloop, client_clone, event_tx).await;
        });

        Self { client }
    }

    /// Create an authenticated client after pairing
    pub fn new_authenticated(
        credentials: &GatewayCredentials,
        config: &EdgeConfig,
        event_tx: mpsc::Sender<MqttEvent>,
    ) -> Self {
        let mut options = MqttOptions::new(&credentials.client_id, &config.mqtt_broker, config.mqtt_port);
        options.set_credentials(&credentials.username, &credentials.password);
        options.set_keep_alive(Duration::from_secs(60));

        let (client, eventloop) = AsyncClient::new(options, 100);
        let client_clone = client.clone();

        tokio::spawn(async move {
            Self::authenticated_event_loop(eventloop, client_clone, event_tx).await;
        });

        Self { client }
    }

    async fn anonymous_event_loop(
        mut eventloop: rumqttc::EventLoop,
        client: AsyncClient,
        event_tx: mpsc::Sender<MqttEvent>,
    ) {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    tracing::info!("Edge MQTT connected (anonymous)");
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    if publish.topic.starts_with("tinyiothub/pairing/") && publish.topic.ends_with("/response") {
                        if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&publish.payload) {
                            let _ = event_tx.send(MqttEvent::PairingAck(msg)).await;
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(?e, "Edge MQTT event loop error (anonymous)");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn authenticated_event_loop(
        mut eventloop: rumqttc::EventLoop,
        client: AsyncClient,
        event_tx: mpsc::Sender<MqttEvent>,
    ) {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    tracing::info!("Edge MQTT connected (authenticated)");
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    if publish.topic.contains("/command") {
                        if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&publish.payload) {
                            let _ = event_tx.send(MqttEvent::Command(msg)).await;
                        }
                    } else if publish.topic.contains("/config") {
                        if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&publish.payload) {
                            let _ = event_tx.send(MqttEvent::Config(msg)).await;
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(?e, "Edge MQTT event loop error (authenticated)");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Publish pairing announce
    pub async fn publish_announce(&self, payload: &[u8]) {
        self.client
            .publish("tinyiothub/pairing/announce", QoS::AtLeastOnce, false, payload)
            .await
            .ok();
    }

    /// Publish gateway status
    pub async fn publish_status(&self, topic: &str, payload: &[u8]) {
        self.client.publish(topic, QoS::AtMostOnce, false, payload).await.ok();
    }

    /// Publish gateway telemetry
    pub async fn publish_telemetry(&self, topic: &str, payload: &[u8]) {
        self.client.publish(topic, QoS::AtMostOnce, false, payload).await.ok();
    }

    /// Publish event
    pub async fn publish_event(&self, topic: &str, payload: &[u8]) {
        self.client.publish(topic, QoS::AtLeastOnce, false, payload).await.ok();
    }

    /// Publish sub-device discovery
    pub async fn publish_discovery(&self, topic: &str, payload: &[u8]) {
        self.client.publish(topic, QoS::AtLeastOnce, false, payload).await.ok();
    }

    /// Subscribe to command and config topics
    pub async fn subscribe_topics(&self, command_topic: &str, config_topic: &str) {
        self.client.subscribe(command_topic, QoS::AtLeastOnce).await.ok();
        self.client.subscribe(config_topic, QoS::AtLeastOnce).await.ok();
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p tinyiothub-edge`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add edge/src/mqtt_client.rs
git commit -m "feat(edge): implement full MQTT client with anonymous pairing and authenticated modes"
```

---

### Task 15: Edge Gateway — Device Discovery

**Files:**
- Create: `edge/src/device_discovery.rs`

- [ ] **Step 1: Write device discovery module**

```rust
// edge/src/device_discovery.rs
use serde::Serialize;

/// Discovered local device (Modbus scan, ONVIF discovery, etc.)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredDevice {
    pub name: String,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub driver_options: Option<String>,
}

/// Device discovery scanner
pub struct DeviceScanner {
    // In v0.1, discovery is manual/config-driven.
    // Future: auto-scan local network for Modbus/ONVIF devices.
    configured_devices: Vec<DiscoveredDevice>,
}

impl DeviceScanner {
    pub fn new() -> Self {
        Self {
            configured_devices: Vec::new(),
        }
    }

    /// Scan for devices. In v0.1, returns configured devices.
    /// v0.2+ will do actual Modbus/ONVIF network scanning.
    pub async fn scan(&self) -> Vec<DiscoveredDevice> {
        // TODO(v0.2): Auto-detect local Modbus TCP devices, ONVIF cameras
        // For now, return configured devices from static config or file
        self.configured_devices.clone()
    }

    /// Load devices from a JSON config file (optional)
    pub fn load_from_config(&mut self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(path)?;
        self.configured_devices = serde_json::from_str(&content)?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDiscoverMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub devices: Vec<DiscoveredDevice>,
}

impl DeviceDiscoverMessage {
    pub fn new(devices: Vec<DiscoveredDevice>) -> Self {
        Self {
            msg_type: "device_discover".into(),
            devices,
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p tinyiothub-edge`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add edge/src/device_discovery.rs
git commit -m "feat(edge): add device discovery scanner"
```

---

### Task 16: Edge Gateway — Main Binary

**Files:**
- Modify: `edge/src/main.rs`
- Modify: `edge/src/runtime.rs` (if needed)

- [ ] **Step 1: Write the edge main.rs entry point**

```rust
// edge/src/main.rs
mod config;
mod device_discovery;
mod mqtt_client;
mod pairing;

use config::{EdgeConfig, GatewayCredentials};
use device_discovery::DeviceScanner;
use mqtt_client::{EdgeMqttClient, MqttEvent};
use pairing::PairingCodeGenerator;
use tokio::sync::mpsc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = EdgeConfig::from_env();
    tracing::info!(?config, "Starting TinyIoTHub Edge Gateway");

    // Check for existing credentials
    let credentials = GatewayCredentials::load(&config.credentials_file);

    if let Some(creds) = credentials {
        tracing::info!(device_id = %creds.device_id, "Found saved credentials, connecting as authenticated");
        run_authenticated(config, creds).await;
    } else {
        tracing::info!("No saved credentials, starting pairing mode");
        run_pairing(config).await;
    }
}

async fn run_pairing(config: EdgeConfig) {
    let (event_tx, mut event_rx) = mpsc::channel::<MqttEvent>(100);
    let mqtt = EdgeMqttClient::new_anonymous(&config, event_tx);
    let mut code_gen = PairingCodeGenerator::new();

    // Get gateway info
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let fingerprint = get_fingerprint();

    loop {
        let code = code_gen.get_code().to_string();
        let display = PairingCodeGenerator::display_format(&code);

        // Show on screen / log
        println!("═══════════════════════════════════");
        println!("  Pairing Code: {display}");
        println!("═══════════════════════════════════");
        tracing::info!(code = %code, "Pairing code displayed");

        // Announce
        let announce = serde_json::json!({
            "type": "pairing_announce",
            "code": code,
            "fingerprint": fingerprint,
            "hostname": hostname,
            "os": std::env::consts::OS,
            "ip": local_ip(),
            "hw_model": "edge-gateway"
        });

        mqtt.publish_announce(serde_json::to_string(&announce).unwrap().as_bytes()).await;

        // Wait for pairing ack or timeout
        let deadline = tokio::time::sleep(Duration::from_secs(config.pairing_interval_secs));
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    if let MqttEvent::PairingAck(ack) = event {
                        if ack.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                            tracing::info!("Pairing successful!");

                            // Save credentials
                            let creds = GatewayCredentials {
                                device_id: ack["device_id"].as_str().unwrap_or_default().to_string(),
                                client_id: ack["credentials"]["client_id"].as_str().unwrap_or_default().to_string(),
                                username: ack["credentials"]["username"].as_str().unwrap_or_default().to_string(),
                                password: ack["credentials"]["password"].as_str().unwrap_or_default().to_string(),
                                workspace_id: ack["workspace_id"].as_str().unwrap_or_default().to_string(),
                            };
                            if let Err(e) = creds.save(&config.credentials_file) {
                                tracing::error!(?e, "Failed to save credentials");
                            }

                            // Reconnect as authenticated
                            run_authenticated(config, creds).await;
                            return;
                        }
                    }
                }
                _ = &mut deadline => {
                    break; // Timeout, re-announce with same code
                }
            }
        }
    }
}

async fn run_authenticated(config: EdgeConfig, creds: GatewayCredentials) {
    let (event_tx, mut event_rx) = mpsc::channel::<MqttEvent>(100);
    let mqtt = EdgeMqttClient::new_authenticated(&creds, &config, event_tx);
    let mut scanner = DeviceScanner::new();

    // Subscribe to command and config topics
    let command_topic = format!("tinyiothub/{}/gateway/{}/command", creds.workspace_id, creds.device_id);
    let config_topic = format!("tinyiothub/{}/gateway/{}/config", creds.workspace_id, creds.device_id);
    mqtt.subscribe_topics(&command_topic, &config_topic).await;

    // Device discovery
    let discover_topic = format!("tinyiothub/{}/gateway/{}/device/discover", creds.workspace_id, creds.device_id);
    let devices = scanner.scan().await;
    if !devices.is_empty() {
        let msg = device_discovery::DeviceDiscoverMessage::new(devices);
        if let Ok(payload) = serde_json::to_string(&msg) {
            mqtt.publish_discovery(&discover_topic, payload.as_bytes()).await;
        }
    }

    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));
    let status_topic = format!("tinyiothub/{}/gateway/{}/status", creds.workspace_id, creds.device_id);
    let telemetry_topic = format!("tinyiothub/{}/gateway/{}/telemetry", creds.workspace_id, creds.device_id);

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                let status = serde_json::json!({
                    "type": "status",
                    "status": "online",
                    "uptime": get_uptime(),
                    "timestamp": chrono::Utc::now().timestamp(),
                });
                mqtt.publish_status(&status_topic, serde_json::to_string(&status).unwrap().as_bytes()).await;
            }
            Some(event) = event_rx.recv() => {
                match event {
                    MqttEvent::Command(cmd) => {
                        tracing::info!(?cmd, "Received command");
                        // Handle command: forward to local device driver
                    }
                    MqttEvent::Config(cfg) => {
                        tracing::info!(?cfg, "Received config update");
                        // Handle config update
                    }
                    _ => {}
                }
            }
        }
    }
}

fn get_fingerprint() -> String {
    // Collect MAC addresses as a simple hardware fingerprint
    mac_address::get_mac_address()
        .ok()
        .flatten()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "0.0.0.0".into())
}

fn get_uptime() -> u64 {
    // Simple uptime in seconds since process start
    0 // Placeholder
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p tinyiothub-edge`
Expected: May error on missing deps `hostname`, `mac_address`, `local_ip_address`. If so, add to edge/Cargo.toml:

```toml
hostname = "0.4"
mac_address = "1.1"
local_ip_address = "0.5"
```

Then re-run check.

- [ ] **Step 3: Commit**

```bash
git add edge/src/main.rs edge/Cargo.toml
git commit -m "feat(edge): wire pairing flow and authenticated mode in main binary"
```

---

### Task 17: Docker — Edge Gateway Image

**Files:**
- Create: `deploy/docker/Dockerfile.edge`

- [ ] **Step 1: Write Dockerfile.edge**

```dockerfile
# deploy/docker/Dockerfile.edge
FROM rust:1.82-slim-bookworm AS builder

WORKDIR /app
COPY . .

RUN cargo build --release -p tinyiothub-edge

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/tinyiothub-edge /usr/local/bin/tinyiothub-edge

RUN mkdir -p /app/data

ENV EDGE_MQTT_BROKER=mqtt.tinyiothub.com
ENV EDGE_MQTT_PORT=1883

ENTRYPOINT ["tinyiothub-edge"]
```

- [ ] **Step 2: Add edge build to CI**

Open `.github/workflows/` and find the Docker build workflow. Add the edge image build:

```yaml
- name: Build and push edge image
  uses: docker/build-push-action@v5
  with:
    context: .
    file: deploy/docker/Dockerfile.edge
    platforms: linux/amd64,linux/arm64
    push: true
    tags: ${{ secrets.DOCKER_HUB_USERNAME }}/tinyiothub-edge:latest
```

- [ ] **Step 3: Commit**

```bash
git add deploy/docker/Dockerfile.edge
git commit -m "feat(edge): add multi-arch Docker image for edge gateway"
```

---

### Task 18: Deployment — Mosquitto ACL and Docker Compose

**Files:**
- Modify: `deploy/docker/mosquitto/config/mosquitto.conf`
- Create: `deploy/docker/mosquitto/config/acl`
- Modify: `deploy/docker/docker-compose.yml`

- [ ] **Step 1: Update mosquitto.conf to allow anonymous with ACL**

```conf
# 允许匿名连接（受 ACL 限制）
allow_anonymous true
password_file /mosquitto/config/passwd
acl_file /mosquitto/config/acl

# Rest of existing config unchanged
```

- [ ] **Step 2: Create ACL file**

```
# deploy/docker/mosquitto/config/acl

# 匿名客户端：只能访问配对 topic
user anonymous
topic readwrite tinyiothub/pairing/#

# 平台客户端：完整访问
user admin
topic readwrite tinyiothub/#

# 网关设备模板（动态下发后，每个网关有独立的 username = device_id）
# 用 pattern 匹配（Mosquitto 2.0 支持 %u 变量）
pattern readwrite tinyiothub/%u/gateway/%u/#
pattern readwrite tinyiothub/%u/device/%u/#
```

- [ ] **Step 3: Update docker-compose.yml to mount ACL**

Add to the `tinyiothub-mqtt` service volumes:

```yaml
- ./mosquitto/config/acl:/mosquitto/config/acl:ro
```

- [ ] **Step 4: Verify the config with Mosquitto locally**

Run: `docker compose -f deploy/docker/docker-compose.yml up -d tinyiothub-mqtt`
Expected: Mosquitto starts and logs show ACL file loaded.

Then test anonymous access restriction:
```bash
# This should succeed (anonymous pairing topic)
mosquitto_pub -h localhost -p 1883 -t "tinyiothub/pairing/announce" -m '{"test":true}'
# This should fail (anonymous can't publish to non-pairing topic)
mosquitto_pub -h localhost -p 1883 -t "tinyiothub/test/status" -m '{"test":true}'
```

- [ ] **Step 5: Commit**

```bash
git add deploy/docker/mosquitto/config/mosquitto.conf deploy/docker/mosquitto/config/acl deploy/docker/docker-compose.yml
git commit -m "feat(mqtt): add Mosquitto ACL for anonymous pairing + admin full access"
```

---

## Spec Coverage Self-Review

| Spec Section | Covered By |
|-------------|------------|
| 配对码规则 & 安全 | Task 4 (PairingCache), Task 5 (Service validation) |
| MQTT Topic 结构 | Task 3 (Types), Task 7 (Platform MQTT), Task 14 (Edge MQTT) |
| 配对协议 (announce/ack) | Task 3 (Types), Task 5 (Service), Task 7 (Platform MQTT) |
| 子设备模型 | Task 5 (Device discover handling), Task 15 (Edge discovery) |
| 数据上报 & 指令下发 | Task 7 (Platform MQTT subscriptions), Task 14 (Edge publish) |
| 错误处理 | Task 5 (PairingError variants), Task 6 (HTTP error codes) |
| 数据库变更 | Task 1 (Migration), Task 2 (Storage) |
| API 变更 | Task 6 (Handler), Task 10 (Frontend API client) |
| 前端配对 UI | Task 10, Task 11 |
| 部署配置 (Mosquitto ACL) | Task 18 |
| 边缘网关 | Task 12-16 |
| Docker 构建 | Task 17 |
| 可观测性 | Task 5 (tracing::info on pairing), Task 7 (tracing on MQTT) |
| e2e 测试 | Covered in TODOS.md (P1 — requires post-implementation) |

## NOT in this plan (per spec scope)

- Gateway OTA firmware upgrade (v0.2+)
- Batch gateway management
- MQTT message persistence / offline queue
- MQTT over TLS on gateway side
- QR/BLE/NFC pairing code formats
- Sub-device removal/deregistration (v0.2)
