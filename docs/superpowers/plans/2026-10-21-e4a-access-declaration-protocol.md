# E4a 接入层声明制（协议先行）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让一个新驱动仅凭 `driver.manifest.json` + 可选 `.so` 接入 TinyIoTHub（Hub 零代码变更），并交付声明校验、双层设备声明、物化、安装 API 与 core/materialization/migration 验收。

**Architecture:** 包级 manifest 是静态契约；设备绑定时驱动 `describe()` 产出实例声明，Hub 校验后持久化到 `device_declarations` 并物化到 `thing_properties`/`thing_actions` 投影。动态驱动只加载 ABI v2；v1 `.so` 拒载并给出迁移指引。E4b-e（采样/资源取消/空间/全套验收）另立计划。

**Tech Stack:** Rust 2024（rust-version 1.85）、serde、schemars 1、sha2、libloading、sqlx（仅 db crate）、axum multipart、tokio。

## Global Constraints

- 上位 spec：`docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md`；任何偏差先改 spec。
- manifest 版本：major=1、minor=0；ABI 版本：2。v1 动态驱动**加载即拒绝**，文案必须包含迁移指引。
- 核心边界：`crates/core` 零 I/O；`crates/runtime` 禁止 sqlx/直接依赖 db；持久化一律新增 `ports.rs` trait，由 `apps/cloud` 适配 `Db` 后注入。
- 事实源：包级 manifest + `device_declarations`；`thing_properties`/`thing_actions` 只是投影。物化只 upsert、不删除；消失项标 `source='stale'`。
- 校验语义：业务参数（schema/range/enum）越界**拒绝**；只对超时**钳制**；未声明工具拒绝。
- 采样/资源仲裁/stop 取消/maps/坐标：**不在本计划**（E4b-e）。
- 默认值：`describe()` 超时 2s、输出 ≤256 KiB；实例声明上限 stateFields=256、tools=128；工具超时默认 3000ms、上限 5000ms（manifest 未声明时）。
- API 沿用现有 `/v1` 路由与 `WorkspaceScope` 注入，不新增 `/workspaces/{ws}` 路径段。
- 每次提交前跑 `cargo fmt --all`；提交信息用 `feat(e4a): ...` / `test(e4a): ...` / `refactor(e4a): ...`。
- 测试命令基线：`cargo test -p core`、`cargo test -p runtime`、`cargo test -p db`、`cargo test -p tinyiothub-cloud`。

---

## File Structure

```
spec/driver-manifest.schema.json                  # gen:check 的单一 JSON Schema（Task 12）
crates/core/src/driver/
  manifest.rs      Manifest v1 类型 + 校验
  declaration.rs   DeviceDeclaration + 双层声明规则 + 持久化记录类型
  schema.rs        最小 JSON Schema 校验器（参数）
  dynamic.rs       ABI v2 类型/常量/迁移文案
  mod.rs           CancelToken + ThingDriver v2 签名
crates/runtime/src/
  access/
    mod.rs         AccessError + 子模块
    package.rs     驱动包读取（manifest+二进制+哈希）
    registry.rs    ManifestRegistry（包级声明）
    device.rs      DeviceDeclarationRegistry（实例声明）
    service.rs     AccessService（安装/绑定/校验/包目录）
    materialize.rs 声明 → 投影
    validator.rs   调用校验/超时钳制 + AccessCallValidator trait
  driver/
    describe.rs    Thing → DeviceDeclaration（内置驱动）
    registry.rs    v2-only 加载
    dynamic_adapter.rs ABI v2 适配
    validator.rs   v2 符号/版本校验
  acceptance/
    mod.rs report.rs runner.rs test_hub.rs profiles/
crates/runtime/manifests/{simulator,modbus_rtu,snmp}.json
crates/db/
  migrations/20261021000002_access_declarations.sql
  src/driver_declaration.rs
crates/driver-check/          # 新 bin crate（workspace glob 覆盖）
drivers/demo-declarative/     # 参考驱动（manifest + cdylib）
apps/cloud/src/domains/driver/declaration/{mod.rs,handler.rs}
apps/cloud/src/shared/access_ports.rs
apps/cloud/src/tests/access_declaration_api_test.rs
```

---

### Task 1: Manifest v1 类型、校验与参数校验器

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `crates/core/src/driver/manifest.rs`
- Create: `crates/core/src/driver/schema.rs`
- Modify: `crates/core/src/driver/mod.rs`
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Produces: `DriverManifest`, `ManifestIdentity`, `ManifestTool`, `StateFieldDecl`, `SourceDecl`, `ResourceDecl`, `PlacementDecl`, `ManifestLimits`, `ManifestError`, `validate_manifest`, `negotiate_manifest_version`, `validate_params`, `MANIFEST_MAJOR/MINOR`, `MANIFEST_FILE`, `DRIVER_ABI_V2`, `MAX_INSTANCE_STATE_FIELDS`, `MAX_INSTANCE_TOOLS`

- [ ] **Step 1: Add schemars and modules**

`crates/core/Cargo.toml`:
```toml
schemars = { version = "1", features = ["derive"] }
```

`crates/core/src/driver/mod.rs`:
```rust
pub mod declaration;
pub mod manifest;
pub mod schema;
```

Create an empty `crates/core/src/driver/declaration.rs` (Task 3 fills it).

- [ ] **Step 2: Write failing tests**

Append to `crates/core/src/driver/manifest.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_manifest_json() -> serde_json::Value {
        json!({
            "manifestVersion": "1.0",
            "identity": {
                "driver": "demo", "version": "1.0.0", "protocol": "demo",
                "displayName": "Demo", "abiVersion": 2, "dynamicCapabilities": false
            },
            "config": { "type": "object", "additionalProperties": true },
            "tools": [{
                "name": "set_level", "kind": "set", "targetField": "level",
                "parameters": {
                    "type": "object",
                    "properties": { "level": { "type": "integer", "minimum": 0, "maximum": 100 } },
                    "required": ["level"], "additionalProperties": false
                },
                "timeout": { "defaultMs": 3000, "maxMs": 5000 },
                "safety": "control", "resources": ["actuator"], "cancellable": true
            }],
            "stateFields": [{
                "name": "level", "type": "integer", "unit": "%",
                "range": { "min": 0, "max": 100 }, "readOnly": false, "required": true,
                "source": "telemetry"
            }],
            "sources": [{
                "id": "telemetry", "kind": "poll", "fields": ["level"],
                "intervalMs": 1000, "minIntervalMs": 200, "demandOnly": false, "health": true
            }],
            "resources": [{ "id": "actuator", "mode": "exclusive", "capacity": 1 }],
            "placement": { "mode": "runtime", "fromSource": "position", "trust": "trusted" },
            "limits": { "maxConcurrentJobs": 1, "maxCallsPerMin": 60 }
        })
    }

    #[test]
    fn parses_valid_manifest() {
        let m: DriverManifest = serde_json::from_value(valid_manifest_json()).unwrap();
        validate_manifest(&m).unwrap();
        assert_eq!(m.tools[0].target_field.as_deref(), Some("level"));
    }

    #[test]
    fn rejects_unsupported_major_version() {
        assert!(matches!(negotiate_manifest_version("2.0"), Err(ManifestError::UnsupportedMajor(2))));
        assert!(negotiate_manifest_version("1.7").is_ok());
        assert!(negotiate_manifest_version("bad").is_err());
    }

    #[test]
    fn unknown_top_level_fields_are_ignored() {
        let mut v = valid_manifest_json();
        v["futureField"] = json!({"anything": true});
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        validate_manifest(&m).unwrap();
    }

    #[test]
    fn rejects_unknown_resource_reference() {
        let mut v = valid_manifest_json();
        v["tools"][0]["resources"] = json!(["missing"]);
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(validate_manifest(&m), Err(ManifestError::UnknownResource { .. })));
    }

    #[test]
    fn rejects_set_tool_whose_target_field_does_not_exist() {
        let mut v = valid_manifest_json();
        v["tools"][0]["targetField"] = json!("nope");
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(validate_manifest(&m), Err(ManifestError::UnknownTargetField { .. })));
    }

    #[test]
    fn rejects_source_field_without_state_field() {
        let mut v = valid_manifest_json();
        v["sources"][0]["fields"] = json!(["missing"]);
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(validate_manifest(&m), Err(ManifestError::UnknownSourceField { .. })));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p core driver::manifest`
Expected: compile error (`DriverManifest` not found).

- [ ] **Step 4: Implement manifest types and validation**

Top of `manifest.rs`:

```rust
use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MANIFEST_MAJOR: u32 = 1;
pub const MANIFEST_MINOR: u32 = 0;
pub const MANIFEST_FILE: &str = "driver.manifest.json";
pub const DRIVER_ABI_V2: u32 = 2;
pub const MAX_INSTANCE_STATE_FIELDS: usize = 256;
pub const MAX_INSTANCE_TOOLS: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DriverManifest {
    pub manifest_version: String,
    pub identity: ManifestIdentity,
    #[serde(default = "default_config_schema")]
    pub config: serde_json::Value,
    #[serde(default)]
    pub capabilities: ManifestCapabilities,
    #[serde(default)]
    pub tools: Vec<ManifestTool>,
    #[serde(default)]
    pub state_fields: Vec<StateFieldDecl>,
    #[serde(default)]
    pub sources: Vec<SourceDecl>,
    #[serde(default)]
    pub resources: Vec<ResourceDecl>,
    #[serde(default)]
    pub placement: PlacementDecl,
    #[serde(default)]
    pub limits: ManifestLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestIdentity {
    pub driver: String,
    pub version: String,
    pub protocol: String,
    pub display_name: Option<String>,
    pub abi_version: u32,
    #[serde(default)]
    pub dynamic_capabilities: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCapabilities {
    #[serde(default)]
    pub cancel: bool,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub streaming: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolKindDecl {
    #[default]
    Action,
    Set,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolSafetyDecl {
    Safe,
    #[default]
    Control,
    Dangerous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeout {
    #[serde(default = "default_timeout_ms")]
    pub default_ms: u64,
    #[serde(default = "default_max_timeout_ms")]
    pub max_ms: u64,
}

fn default_timeout_ms() -> u64 { 3000 }
fn default_max_timeout_ms() -> u64 { 5000 }
fn default_config_schema() -> serde_json::Value { serde_json::json!({"type": "object"}) }
fn default_true() -> bool { true }
fn default_interval_ms() -> u64 { 1000 }
fn default_min_interval_ms() -> u64 { 200 }
fn default_capacity() -> u32 { 1 }
fn default_concurrent() -> u32 { 1 }
fn default_calls_per_min() -> u32 { 60 }

impl Default for ToolTimeout {
    fn default() -> Self {
        Self { default_ms: default_timeout_ms(), max_ms: default_max_timeout_ms() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestTool {
    pub name: String,
    #[serde(default)]
    pub kind: ToolKindDecl,
    #[serde(default)]
    pub target_field: Option<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub timeout: ToolTimeout,
    #[serde(default)]
    pub safety: ToolSafetyDecl,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default = "default_true")]
    pub cancellable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NumericRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StateFieldDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub range: Option<NumericRange>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceDecl {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_min_interval_ms")]
    pub min_interval_ms: u64,
    #[serde(default)]
    pub demand_only: bool,
    #[serde(default)]
    pub health: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResourceMode {
    #[default]
    Exclusive,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDecl {
    pub id: String,
    #[serde(default)]
    pub mode: ResourceMode,
    #[serde(default = "default_capacity")]
    pub capacity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlacementMode {
    Fixed,
    #[default]
    Runtime,
    Declared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum LocationTrustDecl {
    #[default]
    Trusted,
    Uncertain,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlacementDecl {
    #[serde(default)]
    pub mode: PlacementMode,
    #[serde(default)]
    pub from_source: Option<String>,
    #[serde(default)]
    pub map_ref: Option<String>,
    #[serde(default)]
    pub zone_ref: Option<String>,
    #[serde(default)]
    pub position: Option<serde_json::Value>,
    #[serde(default)]
    pub trust: LocationTrustDecl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestLimits {
    #[serde(default = "default_concurrent")]
    pub max_concurrent_jobs: u32,
    #[serde(default = "default_calls_per_min")]
    pub max_calls_per_min: u32,
}

impl Default for ManifestLimits {
    fn default() -> Self {
        Self { max_concurrent_jobs: default_concurrent(), max_calls_per_min: default_calls_per_min() }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest version '{0}' must be '<major>.<minor>'")]
    InvalidVersion(String),
    #[error("unsupported manifest major version {0} (hub supports {MANIFEST_MAJOR})")]
    UnsupportedMajor(u32),
    #[error("unsupported ABI version {0} (requires {DRIVER_ABI_V2})")]
    UnsupportedAbi(u32),
    #[error("identity.driver must not be empty")]
    EmptyDriver,
    #[error("duplicate {kind}: {name}")]
    Duplicate { kind: &'static str, name: String },
    #[error("tool '{0}' parameters must be an object schema")]
    InvalidToolSchema(String),
    #[error("set tool '{tool}' must declare targetField")]
    MissingTargetField { tool: String },
    #[error("set tool '{tool}' references unknown target field '{field}'")]
    UnknownTargetField { tool: String, field: String },
    #[error("tool '{tool}' references unknown resource '{resource}'")]
    UnknownResource { tool: String, resource: String },
    #[error("source '{source}' references unknown state field '{field}'")]
    UnknownSourceField { source_id: String, field: String },
    #[error("limits must be greater than zero")]
    InvalidLimits,
    #[error("declaration identity mismatch: expected {expected}, got {got}")]
    IdentityMismatch { expected: String, got: String },
    #[error("too many {kind}: {count} (max {max})")]
    TooMany { kind: &'static str, count: usize, max: usize },
}

pub fn negotiate_manifest_version(v: &str) -> Result<(u32, u32), ManifestError> {
    let (major, minor) = v.split_once('.').ok_or_else(|| ManifestError::InvalidVersion(v.to_string()))?;
    let major: u32 = major.parse().map_err(|_| ManifestError::InvalidVersion(v.to_string()))?;
    let minor: u32 = minor.parse().map_err(|_| ManifestError::InvalidVersion(v.to_string()))?;
    if major != MANIFEST_MAJOR {
        return Err(ManifestError::UnsupportedMajor(major));
    }
    Ok((major, minor))
}

pub fn validate_manifest(m: &DriverManifest) -> Result<(), ManifestError> {
    negotiate_manifest_version(&m.manifest_version)?;
    if m.identity.abi_version != DRIVER_ABI_V2 {
        return Err(ManifestError::UnsupportedAbi(m.identity.abi_version));
    }
    if m.identity.driver.trim().is_empty() {
        return Err(ManifestError::EmptyDriver);
    }
    if m.limits.max_concurrent_jobs == 0 || m.limits.max_calls_per_min == 0 {
        return Err(ManifestError::InvalidLimits);
    }

    let mut names = HashSet::new();
    for f in &m.state_fields {
        if !names.insert(f.name.clone()) {
            return Err(ManifestError::Duplicate { kind: "state field", name: f.name.clone() });
        }
    }
    let mut tool_names = HashSet::new();
    let resource_ids: HashSet<&str> = m.resources.iter().map(|r| r.id.as_str()).collect();
    for t in &m.tools {
        if !tool_names.insert(t.name.clone()) {
            return Err(ManifestError::Duplicate { kind: "tool", name: t.name.clone() });
        }
        if !t.parameters.is_object() {
            return Err(ManifestError::InvalidToolSchema(t.name.clone()));
        }
        for r in &t.resources {
            if !resource_ids.contains(r.as_str()) {
                return Err(ManifestError::UnknownResource { tool: t.name.clone(), resource: r.clone() });
            }
        }
        if t.kind == ToolKindDecl::Set {
            let field = t.target_field.as_ref().ok_or_else(|| ManifestError::MissingTargetField {
                tool: t.name.clone(),
            })?;
            let decl = m.state_fields.iter().find(|f| &f.name == field).ok_or_else(|| {
                ManifestError::UnknownTargetField { tool: t.name.clone(), field: field.clone() }
            })?;
            if decl.read_only {
                return Err(ManifestError::UnknownTargetField { tool: t.name.clone(), field: field.clone() });
            }
        }
    }
    for s in &m.sources {
        for f in &s.fields {
            if !names.contains(f) {
                return Err(ManifestError::UnknownSourceField { source_id: s.id.clone(), field: f.clone() });
            }
        }
    }
    Ok(())
}
```

`schema.rs` (full implementation as in the validated subset):

```rust
//! Minimal JSON Schema validator for declared tool parameters (v1 subset:
//! type / properties / required / additionalProperties / enum / min / max).

use serde_json::Value;

pub fn validate_params(schema: &Value, params: &Value) -> Result<(), String> {
    let obj = schema.as_object().ok_or("schema must be a JSON object")?;
    match obj.get("type").and_then(Value::as_str) {
        Some("object") => validate_object(obj, params),
        Some(other) => validate_value(other, obj, params),
        None => Ok(()),
    }
}

fn validate_object(schema: &serde_json::Map<String, Value>, params: &Value) -> Result<(), String> {
    let params = params.as_object().ok_or("expected object")?;
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for r in required {
            let name = r.as_str().ok_or("required entries must be strings")?;
            if !params.contains_key(name) {
                return Err(format!("missing required field: {name}"));
            }
        }
    }
    let props = schema.get("properties").and_then(Value::as_object);
    let deny_unknown = schema.get("additionalProperties").and_then(Value::as_bool) == Some(false);
    for (key, value) in params {
        match props.and_then(|p| p.get(key)) {
            Some(prop_schema) => {
                let pobj = prop_schema.as_object().ok_or("property schema must be an object")?;
                let ty = pobj.get("type").and_then(Value::as_str).unwrap_or("");
                validate_value(ty, pobj, value).map_err(|e| format!("{key}: {e}"))?;
            }
            None if deny_unknown => return Err(format!("unknown field: {key}")),
            None => {}
        }
    }
    Ok(())
}

fn validate_value(ty: &str, schema: &serde_json::Map<String, Value>, value: &Value) -> Result<(), String> {
    match ty {
        "string" if !value.is_string() => return Err("expected string".into()),
        "boolean" if !value.is_boolean() => return Err("expected boolean".into()),
        "integer" if !value.is_i64() && !value.is_u64() => return Err("expected integer".into()),
        "number" if !value.is_number() => return Err("expected number".into()),
        "array" if !value.is_array() => return Err("expected array".into()),
        "object" if !value.is_object() => return Err("expected object".into()),
        _ => {}
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err("value not in enum".into());
    }
    if let Some(n) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64) && n < min {
            return Err(format!("below minimum {min}"));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64) && n > max {
            return Err(format!("above maximum {max}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_valid_object() {
        let schema = json!({"type":"object","properties":{"level":{"type":"integer","minimum":0,"maximum":100}},"required":["level"],"additionalProperties":false});
        validate_params(&schema, &json!({"level": 50})).unwrap();
    }

    #[test]
    fn rejects_missing_required() {
        let schema = json!({"type":"object","properties":{"level":{"type":"integer"}},"required":["level"]});
        assert!(validate_params(&schema, &json!({})).unwrap_err().contains("missing required"));
    }

    #[test]
    fn rejects_range_violation() {
        let schema = json!({"type":"object","properties":{"level":{"type":"integer","minimum":0,"maximum":100}}});
        assert!(validate_params(&schema, &json!({"level": 101})).unwrap_err().contains("above maximum"));
    }

    #[test]
    fn rejects_unknown_field_when_additional_properties_false() {
        let schema = json!({"type":"object","properties":{},"additionalProperties":false});
        assert!(validate_params(&schema, &json!({"x":1})).unwrap_err().contains("unknown field"));
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p core driver::`
Expected: PASS (10 new tests + pre-existing driver tests).

- [ ] **Step 6: Commit**

```bash
git add crates/core/Cargo.toml crates/core/src/driver/mod.rs crates/core/src/driver/manifest.rs crates/core/src/driver/schema.rs crates/core/src/driver/declaration.rs
git commit -m "feat(e4a): manifest v1 类型、校验与参数 schema 校验器"
```

---

### Task 2: ABI v2 契约与 v1 迁移文案

**Files:**
- Modify: `crates/core/src/driver/dynamic.rs`
- Test: inline

**Interfaces:**
- Produces: `DRIVER_ABI_VERSION_V2`, `SYMBOL_VTABLE_V2`, `CancelHandle`, `DriverDone`, `DriverVTableV2`, `v1_unsupported_message`, `verify_v2_version`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_version_must_match() {
        assert!(verify_v2_version(2).is_ok());
        let err = verify_v2_version(1).unwrap_err();
        assert!(err.contains("ABI v1 is unsupported"));
        assert!(err.contains("async") && err.contains("cancel") && err.contains("describe"));
    }

    #[test]
    fn v1_message_names_the_artifact() {
        let msg = v1_unsupported_message(std::path::Path::new("/tmp/libfoo.so"));
        assert!(msg.contains("libfoo.so"));
        assert!(msg.contains("tinyiothub_driver_vtable_v2"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p core driver::dynamic`
Expected: compile error.

- [ ] **Step 3: Implement ABI v2**

Add:

```rust
pub const DRIVER_ABI_VERSION_V2: u32 = 2;
pub const SYMBOL_VTABLE_V2: &[u8] = b"tinyiothub_driver_vtable_v2\0";

#[repr(C)]
pub struct CancelHandle {
    pub is_cancelled: extern "C" fn(ctx: *mut c_void) -> c_int,
    pub ctx: *mut c_void,
}

/// status: 0 ok, 1 error, 2 cancelled. out_json is driver-allocated;
/// the hub frees it via `free_string`.
pub type DriverDone = extern "C" fn(user_data: *mut c_void, status: c_int, out_json: *mut c_char);

#[repr(C)]
pub struct DriverVTableV2 {
    pub version: u32,
    pub read_async: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        sources_json: *const c_char,
        cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> c_int,
    pub execute_async: extern "C" fn(
        ctx: *mut c_void,
        config_json: *const c_char,
        cmd_json: *const c_char,
        cancel: *mut CancelHandle,
        done: DriverDone,
        user_data: *mut c_void,
    ) -> c_int,
    pub set_demand: extern "C" fn(ctx: *mut c_void, demand_json: *const c_char) -> c_int,
    pub describe: extern "C" fn(ctx: *mut c_void, config_json: *const c_char, out_json: *mut *mut c_char) -> c_int,
    pub free_string: extern "C" fn(s: *mut c_char),
}

pub fn verify_v2_version(version: u32) -> Result<(), String> {
    if version == DRIVER_ABI_VERSION_V2 {
        Ok(())
    } else {
        Err(format!(
            "ABI v{version} is unsupported by E4 hub; migrate to v2 (async read/execute + cancel + describe) \
             and export symbol tinyiothub_driver_vtable_v2"
        ))
    }
}

pub fn v1_unsupported_message(path: &std::path::Path) -> String {
    format!(
        "driver '{}' does not export tinyiothub_driver_vtable_v2; ABI v1 is unsupported by E4. \
         Migrate to ABI v2 (async read/execute + cancel + describe). See \
         docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md §2.3.",
        path.display()
    )
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p core driver::dynamic`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/driver/dynamic.rs
git commit -m "feat(e4a): ABI v2 契约与 v1 迁移文案"
```

---

### Task 3: 双层设备声明与持久化记录类型

**Files:**
- Modify: `crates/core/src/driver/declaration.rs`
- Modify: `crates/core/src/driver/mod.rs`
- Test: inline

**Interfaces:**
- Produces: `DeviceDeclaration`, `fixed_declaration`, `validate_device_declaration`, `DriverDeclarationRecord`, `DeviceDeclarationRecord`, `AcceptanceReportRecord`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::manifest::*;
    use serde_json::json;

    fn package(dynamic: bool) -> DriverManifest {
        serde_json::from_value(json!({
            "manifestVersion":"1.0",
            "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":dynamic},
            "config":{"type":"object"},"tools":[],"stateFields":[],
            "sources":[{"id":"telemetry","kind":"poll","fields":[],"intervalMs":1000,"minIntervalMs":200}],
            "resources":[],"placement":{"mode":"runtime"},
            "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
        })).unwrap()
    }

    fn decl() -> DeviceDeclaration {
        DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![StateFieldDecl {
                name: "level".into(), data_type: "integer".into(), unit: Some("%".into()),
                range: Some(NumericRange { min: Some(0.0), max: Some(100.0) }),
                read_only: false, required: true, source: Some("telemetry".into()),
            }],
            tools: vec![ManifestTool {
                name: "set_level".into(), kind: ToolKindDecl::Set,
                target_field: Some("level".into()),
                parameters: json!({"type":"object","properties":{"level":{"type":"integer"}},"required":["level"]}),
                timeout: ToolTimeout::default(), safety: ToolSafetyDecl::Control,
                resources: vec![], cancellable: true,
            }],
        }
    }

    #[test]
    fn fixed_declaration_copies_package_capabilities() {
        let mut p = package(false);
        p.state_fields = decl().state_fields;
        p.tools = decl().tools;
        let d = fixed_declaration(&p);
        assert_eq!(d.state_fields[0].name, "level");
        validate_device_declaration(&p, &d).unwrap();
    }

    #[test]
    fn dynamic_declaration_may_add_fields_and_tools() {
        validate_device_declaration(&package(true), &decl()).unwrap();
    }

    #[test]
    fn rejects_identity_mismatch() {
        let p = package(true);
        let mut d = decl();
        d.driver_version = "9.9.9".into();
        assert!(matches!(
            validate_device_declaration(&p, &d),
            Err(ManifestError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn rejects_tool_limit_overflow() {
        let p = package(true);
        let mut d = decl();
        d.tools = (0..=MAX_INSTANCE_TOOLS)
            .map(|i| ManifestTool { name: format!("t{i}"), ..d.tools[0].clone() })
            .collect();
        assert!(matches!(
            validate_device_declaration(&p, &d),
            Err(ManifestError::TooMany { .. })
        ));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p core driver::declaration`
Expected: compile error.

- [ ] **Step 3: Implement**

```rust
//! Device-level declaration (spec §0 #3): package manifest + describe().

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::manifest::{
    DriverManifest, ManifestError, ManifestTool, StateFieldDecl, MAX_INSTANCE_STATE_FIELDS,
    MAX_INSTANCE_TOOLS,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDeclaration {
    pub manifest_version: String,
    pub driver: String,
    pub driver_version: String,
    #[serde(default)]
    pub state_fields: Vec<StateFieldDecl>,
    #[serde(default)]
    pub tools: Vec<ManifestTool>,
}

pub fn fixed_declaration(m: &DriverManifest) -> DeviceDeclaration {
    DeviceDeclaration {
        manifest_version: m.manifest_version.clone(),
        driver: m.identity.driver.clone(),
        driver_version: m.identity.version.clone(),
        state_fields: m.state_fields.clone(),
        tools: m.tools.clone(),
    }
}

pub fn validate_device_declaration(
    package: &DriverManifest,
    decl: &DeviceDeclaration,
) -> Result<(), ManifestError> {
    if decl.driver != package.identity.driver || decl.driver_version != package.identity.version {
        return Err(ManifestError::IdentityMismatch {
            expected: format!("{}@{}", package.identity.driver, package.identity.version),
            got: format!("{}@{}", decl.driver, decl.driver_version),
        });
    }
    if decl.state_fields.len() > MAX_INSTANCE_STATE_FIELDS {
        return Err(ManifestError::TooMany {
            kind: "state fields", count: decl.state_fields.len(), max: MAX_INSTANCE_STATE_FIELDS,
        });
    }
    if decl.tools.len() > MAX_INSTANCE_TOOLS {
        return Err(ManifestError::TooMany {
            kind: "tools", count: decl.tools.len(), max: MAX_INSTANCE_TOOLS,
        });
    }

    let mut fields = HashSet::new();
    for f in &decl.state_fields {
        if !fields.insert(f.name.as_str()) {
            return Err(ManifestError::Duplicate { kind: "state field", name: f.name.clone() });
        }
    }
    let mut tool_names = HashSet::new();
    for t in &decl.tools {
        if !tool_names.insert(t.name.as_str()) {
            return Err(ManifestError::Duplicate { kind: "tool", name: t.name.clone() });
        }
        if !t.parameters.is_object() {
            return Err(ManifestError::InvalidToolSchema(t.name.clone()));
        }
        if t.kind == super::manifest::ToolKindDecl::Set {
            let field = t.target_field.as_ref().ok_or_else(|| ManifestError::MissingTargetField {
                tool: t.name.clone(),
            })?;
            let f = decl.state_fields.iter().find(|f| &f.name == field).ok_or_else(|| {
                ManifestError::UnknownTargetField { tool: t.name.clone(), field: field.clone() }
            })?;
            if f.read_only {
                return Err(ManifestError::UnknownTargetField { tool: t.name.clone(), field: field.clone() });
            }
        }
    }
    for s in &package.sources {
        for f in &s.fields {
            if !fields.contains(f.as_str()) {
                return Err(ManifestError::UnknownSourceField { source_id: s.id.clone(), field: f.clone() });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DriverDeclarationRecord {
    pub workspace_id: String,
    pub driver_name: String,
    pub driver_version: String,
    pub manifest_version: String,
    pub manifest_json: String,
    pub manifest_hash: String,
    pub binary_hash: Option<String>,
    pub status: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceDeclarationRecord {
    pub workspace_id: String,
    pub thing_id: String,
    pub driver_name: String,
    pub driver_version: String,
    pub declaration_json: String,
    pub declaration_hash: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceReportRecord {
    pub workspace_id: String,
    pub driver_name: String,
    pub driver_version: String,
    pub profile: String,
    pub level: String,
    pub report_json: String,
    pub created_at: String,
}
```

`crates/core/src/driver/mod.rs` re-exports:
```rust
pub use declaration::{
    AcceptanceReportRecord, DeviceDeclaration, DeviceDeclarationRecord, DriverDeclarationRecord,
    fixed_declaration, validate_device_declaration,
};
pub use manifest::{DriverManifest, ManifestError, validate_manifest};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p core driver::declaration`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/driver/declaration.rs crates/core/src/driver/mod.rs
git commit -m "feat(e4a): 双层设备声明与校验"
```

---

### Task 4: 持久化表、port trait 与 Db 实现

**Files:**
- Create: `crates/db/migrations/20261021000002_access_declarations.sql`
- Create: `crates/db/src/driver_declaration.rs`
- Modify: `crates/db/src/lib.rs`
- Modify: `crates/runtime/src/ports.rs`
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Produces: `DriverDeclarationStore`, `DeviceDeclarationStore`, `AcceptanceReportStore`, `DeclarationProjectionStore`, `ThingDeclarationSource`; Db methods `upsert_driver_declaration`, `list_active_driver_declarations`, `list_all_driver_declarations`, `upsert_device_declaration`, `list_device_declarations`, `find_device_declaration`, `insert_acceptance_report`, `upsert_declared_properties`, `upsert_declared_commands`, `mark_stale_declared_capabilities`, `find_thing_with_properties`

- [ ] **Step 1: Migration**

`crates/db/migrations/20261021000002_access_declarations.sql`:

```sql
CREATE TABLE driver_declarations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    driver_version TEXT NOT NULL,
    manifest_version TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    binary_hash TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    installed_at TEXT NOT NULL,
    UNIQUE(workspace_id, driver_name, driver_version)
);
CREATE INDEX idx_driver_declarations_active
    ON driver_declarations(workspace_id, driver_name, status);

CREATE TABLE device_declarations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    thing_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    driver_version TEXT NOT NULL,
    declaration_json TEXT NOT NULL,
    declaration_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    updated_at TEXT NOT NULL,
    UNIQUE(thing_id, driver_name)
);
CREATE INDEX idx_device_declarations_ws ON device_declarations(workspace_id);

CREATE TABLE acceptance_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    driver_version TEXT NOT NULL,
    profile TEXT NOT NULL,
    level TEXT NOT NULL,
    report_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_acceptance_reports_driver
    ON acceptance_reports(workspace_id, driver_name);

ALTER TABLE thing_actions ADD COLUMN source TEXT NOT NULL DEFAULT 'template';
```

- [ ] **Step 2: Failing tests**

Append to `crates/db/src/driver_declaration.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::test_pool;
    use tinyiothub_core::driver::declaration::{
        DeviceDeclarationRecord, DriverDeclarationRecord,
    };
    use tinyiothub_core::driver::manifest::{NumericRange, StateFieldDecl};

    fn driver_rec() -> DriverDeclarationRecord {
        DriverDeclarationRecord {
            workspace_id: "ws1".into(), driver_name: "demo".into(), driver_version: "1.0.0".into(),
            manifest_version: "1.0".into(), manifest_json: "{}".into(), manifest_hash: "h1".into(),
            binary_hash: Some("b1".into()), status: "active".into(),
            installed_at: "2026-10-21T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn upserts_and_retires_old_versions() {
        let db = Db::new(test_pool().await);
        db.upsert_driver_declaration(&driver_rec()).await.unwrap();
        let mut r2 = driver_rec();
        r2.driver_version = "1.1.0".into();
        r2.manifest_hash = "h2".into();
        db.upsert_driver_declaration(&r2).await.unwrap();
        let active = db.list_active_driver_declarations("ws1").await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].driver_version, "1.1.0");
        assert_eq!(active[0].manifest_hash, "h2");
    }

    #[tokio::test]
    async fn upserts_declared_properties_and_marks_stale() {
        let db = Db::new(test_pool().await);
        sqlx::query(
            "INSERT INTO things (id, name, thing_type, state, workspace_id, created_at, updated_at) \
             VALUES ('t1','t1','device',0,'ws1','2026-10-21','2026-10-21')",
        )
        .execute(db.pool()).await.unwrap();
        let fields = vec![StateFieldDecl {
            name: "level".into(), data_type: "integer".into(), unit: Some("%".into()),
            range: Some(NumericRange { min: Some(0.0), max: Some(100.0) }),
            read_only: false, required: true, source: Some("telemetry".into()),
        }];
        db.upsert_declared_properties("t1", &fields).await.unwrap();
        db.mark_stale_declared_capabilities("t1", &[], &[]).await.unwrap();
        let props = db.find_thing_properties_by_thing_id("t1").await.unwrap();
        assert_eq!(props[0].name, "level");
        assert_eq!(props[0].source.as_deref(), Some("stale"));
    }

    #[tokio::test]
    async fn device_declaration_round_trip() {
        let db = Db::new(test_pool().await);
        let rec = DeviceDeclarationRecord {
            workspace_id: "ws1".into(), thing_id: "t1".into(), driver_name: "demo".into(),
            driver_version: "1.0.0".into(), declaration_json: "{}".into(),
            declaration_hash: "dh".into(), status: "active".into(), updated_at: "2026-10-21".into(),
        };
        db.upsert_device_declaration(&rec).await.unwrap();
        let got = db.find_device_declaration("ws1", "t1").await.unwrap().unwrap();
        assert_eq!(got.declaration_hash, "dh");
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p db driver_declaration`
Expected: compile error.

- [ ] **Step 4: Add port traits**

Append to `crates/runtime/src/ports.rs`:

```rust
use tinyiothub_core::driver::declaration::{
    AcceptanceReportRecord, DeviceDeclarationRecord, DriverDeclarationRecord,
};
use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};

#[async_trait]
pub trait DriverDeclarationStore: Send + Sync {
    async fn upsert_active(&self, row: &DriverDeclarationRecord) -> Result<(), String>;
    async fn list_active(&self, workspace_id: &str) -> Result<Vec<DriverDeclarationRecord>, String>;
    async fn list_all(&self) -> Result<Vec<DriverDeclarationRecord>, String>;
}

#[async_trait]
pub trait DeviceDeclarationStore: Send + Sync {
    async fn upsert(&self, row: &DeviceDeclarationRecord) -> Result<(), String>;
    async fn list_by_workspace(&self, workspace_id: &str) -> Result<Vec<DeviceDeclarationRecord>, String>;
    async fn find(&self, workspace_id: &str, thing_id: &str) -> Result<Option<DeviceDeclarationRecord>, String>;
}

#[async_trait]
pub trait AcceptanceReportStore: Send + Sync {
    async fn insert(&self, row: &AcceptanceReportRecord) -> Result<(), String>;
}

#[async_trait]
pub trait DeclarationProjectionStore: Send + Sync {
    async fn upsert_properties(&self, thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String>;
    async fn upsert_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), String>;
    async fn mark_stale(&self, thing_id: &str, keep_properties: &[String], keep_commands: &[String])
        -> Result<(), String>;
}

#[async_trait]
pub trait ThingDeclarationSource: Send + Sync {
    async fn load_thing(&self, thing_id: &str) -> Result<Option<Thing>, String>;
}
```

- [ ] **Step 5: Implement Db functions**

`crates/db/src/driver_declaration.rs`:

```rust
//! E4a driver/device declarations + projection writes.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use tinyiothub_core::driver::declaration::{
    AcceptanceReportRecord, DeviceDeclarationRecord, DriverDeclarationRecord,
};
use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};
use crate::Db;

pub(crate) async fn upsert_driver_declaration(
    pool: &SqlitePool, r: &DriverDeclarationRecord,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE driver_declarations SET status='retired' \
         WHERE workspace_id=? AND driver_name=? AND driver_version<>?",
    )
    .bind(&r.workspace_id).bind(&r.driver_name).bind(&r.driver_version)
    .execute(pool).await?;
    sqlx::query(
        r#"INSERT INTO driver_declarations
             (workspace_id, driver_name, driver_version, manifest_version,
              manifest_json, manifest_hash, binary_hash, status, installed_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(workspace_id, driver_name, driver_version) DO UPDATE SET
             manifest_version=excluded.manifest_version, manifest_json=excluded.manifest_json,
             manifest_hash=excluded.manifest_hash, binary_hash=excluded.binary_hash,
             status=excluded.status, installed_at=excluded.installed_at"#,
    )
    .bind(&r.workspace_id).bind(&r.driver_name).bind(&r.driver_version)
    .bind(&r.manifest_version).bind(&r.manifest_json).bind(&r.manifest_hash)
    .bind(&r.binary_hash).bind(&r.status).bind(&r.installed_at)
    .execute(pool).await?;
    Ok(())
}
```

Implement list/find with `sqlx::Row` mapping (bind `workspace_id`, read columns with `row.try_get`), `upsert_device_declaration` (ON CONFLICT(thing_id, driver_name)), `insert_acceptance_report`, and the three projection functions exactly as specified in the spec:

```rust
pub(crate) async fn upsert_declared_properties(
    pool: &SqlitePool, thing_id: &str, fields: &[StateFieldDecl],
) -> Result<(), sqlx::Error> {
    for f in fields {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        sqlx::query(
            r#"INSERT INTO thing_properties
                 (id, thing_id, name, display_name, description, data_type, unit,
                  min_value, max_value, default_value, is_read_only, source, created_at, updated_at)
               VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, NULL, ?, 'driver', ?, ?)
               ON CONFLICT(thing_id, name) DO UPDATE SET
                 display_name=excluded.display_name, data_type=excluded.data_type,
                 unit=excluded.unit, min_value=excluded.min_value,
                 max_value=excluded.max_value, is_read_only=excluded.is_read_only,
                 source='driver', updated_at=excluded.updated_at"#,
        )
        .bind(&id).bind(thing_id).bind(&f.name).bind(&f.name)
        .bind(&f.data_type).bind(&f.unit)
        .bind(f.range.as_ref().and_then(|r| r.min))
        .bind(f.range.as_ref().and_then(|r| r.max))
        .bind(if f.read_only { 1i32 } else { 0i32 })
        .bind(&now).bind(&now).execute(pool).await?;
    }
    Ok(())
}

pub(crate) async fn upsert_declared_commands(
    pool: &SqlitePool, thing_id: &str, tools: &[ManifestTool],
) -> Result<(), sqlx::Error> {
    for t in tools {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let params = serde_json::to_string(&t.parameters).unwrap_or_else(|_| "{}".into());
        sqlx::query(
            r#"INSERT INTO thing_actions
                 (id, thing_id, name, display_name, description, parameters, created_at, updated_at, source)
               VALUES (?, ?, ?, ?, NULL, ?, ?, ?, 'driver')
               ON CONFLICT(thing_id, name) DO UPDATE SET
                 parameters=excluded.parameters, source='driver', updated_at=excluded.updated_at"#,
        )
        .bind(&id).bind(thing_id).bind(&t.name).bind(&t.name)
        .bind(&params).bind(&now).bind(&now).execute(pool).await?;
    }
    Ok(())
}

pub(crate) async fn mark_stale_declared_capabilities(
    pool: &SqlitePool, thing_id: &str, keep_properties: &[String], keep_commands: &[String],
) -> Result<(), sqlx::Error> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    mark_stale(pool, "thing_properties", thing_id, keep_properties, now.clone()).await?;
    mark_stale(pool, "thing_actions", thing_id, keep_commands, now).await
}

async fn mark_stale(
    pool: &SqlitePool, table: &str, thing_id: &str, keep: &[String], now: String,
) -> Result<(), sqlx::Error> {
    if keep.is_empty() {
        let sql = format!("UPDATE {table} SET source='stale', updated_at=? WHERE thing_id=? AND source='driver'");
        sqlx::query(&sql).bind(&now).bind(thing_id).execute(pool).await?;
    } else {
        let placeholders = std::iter::repeat("?").take(keep.len()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE {table} SET source='stale', updated_at=? WHERE thing_id=? AND source='driver' AND name NOT IN ({placeholders})"
        );
        let mut q = sqlx::query(&sql).bind(&now).bind(thing_id);
        for name in keep { q = q.bind(name); }
        q.execute(pool).await?;
    }
    Ok(())
}
```

Add `impl Db` delegates for all functions, and `find_thing_with_properties` if absent (reuse the existing free function used by `DerivedPropertyQueries`). Add `pub mod driver_declaration;` to `crates/db/src/lib.rs`.

- [ ] **Step 6: Run tests**

Run: `cargo test -p db driver_declaration`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
git add crates/db/migrations/20261021000002_access_declarations.sql crates/db/src/driver_declaration.rs crates/db/src/lib.rs crates/runtime/src/ports.rs
git commit -m "feat(e4a): 声明持久化表、port trait 与 Db 实现"
```

---

### Task 5: 驱动包读取与 ManifestRegistry

**Files:**
- Create: `crates/runtime/src/access/{mod.rs,package.rs,registry.rs}`
- Modify: `crates/runtime/src/lib.rs`
- Modify: `crates/runtime/Cargo.toml` (`sha2`; `tempfile` dev-dep)
- Test: inline

**Interfaces:**
- Produces: `AccessError`, `LoadedPackage`, `read_package`, `ManifestRegistry::{new, seed_builtin, install, install_dir, get, list}`, `InstalledPackage`

- [ ] **Step 1: Dependencies and modules**

`crates/runtime/Cargo.toml` add `sha2 = { workspace = true }`; dev-deps add `tempfile = { workspace = true }`.
`crates/runtime/src/lib.rs` add `pub mod access;`.

- [ ] **Step 2: Failing tests**

`package.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],
      "stateFields":[{"name":"level","type":"integer"}],
      "sources":[{"id":"telemetry","kind":"poll","fields":["level"]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    #[test]
    fn reads_manifest_and_hashes_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("driver.manifest.json"), MANIFEST).unwrap();
        let pkg = read_package(dir.path()).unwrap();
        assert_eq!(pkg.manifest.identity.driver, "demo");
        assert_eq!(pkg.manifest_hash.len(), 64);
        assert!(pkg.binary_path.is_none());
    }

    #[test]
    fn rejects_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(read_package(dir.path()), Err(AccessError::MissingManifest(_))));
    }

    #[test]
    fn rejects_unsupported_major() {
        let dir = tempfile::tempdir().unwrap();
        let bad = MANIFEST.replace("\"manifestVersion\":\"1.0\"", "\"manifestVersion\":\"2.0\"");
        fs::write(dir.path().join("driver.manifest.json"), bad).unwrap();
        assert!(matches!(read_package(dir.path()), Err(AccessError::Manifest(_))));
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p runtime access::package`
Expected: compile error.

- [ ] **Step 4: Implement**

`access/mod.rs`:

```rust
pub mod device;
pub mod materialize;
pub mod package;
pub mod registry;
pub mod service;
pub mod validator;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("missing driver.manifest.json in {0}")]
    MissingManifest(PathBuf),
    #[error("manifest error: {0}")]
    Manifest(#[from] tinyiothub_core::driver::manifest::ManifestError),
    #[error("manifest JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("driver '{0}' not installed in workspace '{1}'")]
    NotFound(String, String),
    #[error("ABI error: {0}")]
    Abi(String),
    #[error("store error: {0}")]
    Store(String),
}
```

(Create `device.rs`, `materialize.rs`, `service.rs`, `validator.rs` as empty files so `mod.rs` compiles; Tasks 6/10/11 fill them.)

`package.rs`:

```rust
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use tinyiothub_core::driver::manifest::{DriverManifest, MANIFEST_FILE, validate_manifest};
use super::AccessError;

#[derive(Debug, Clone)]
pub struct LoadedPackage {
    pub manifest: DriverManifest,
    pub manifest_json: String,
    pub manifest_hash: String,
    pub binary_path: Option<PathBuf>,
    pub binary_hash: Option<String>,
}

pub fn read_package(dir: &Path) -> Result<LoadedPackage, AccessError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Err(AccessError::MissingManifest(dir.to_path_buf()));
    }
    let manifest_json = std::fs::read_to_string(&manifest_path)?;
    let manifest: DriverManifest = serde_json::from_str(&manifest_json)?;
    validate_manifest(&manifest)?;
    let manifest_hash = hex_sha256(manifest_json.as_bytes());
    let binary_path = find_binary(dir);
    let binary_hash = match &binary_path {
        Some(p) => Some(hex_sha256(&std::fs::read(p)?)),
        None => None,
    };
    Ok(LoadedPackage { manifest, manifest_json, manifest_hash, binary_path, binary_hash })
}

fn find_binary(dir: &Path) -> Option<PathBuf> {
    let candidates = ["driver.so", "driver.dylib", "driver.dll", "libdriver.so", "libdriver.dylib"];
    for c in candidates {
        let p = dir.join(c);
        if p.is_file() { return Some(p); }
    }
    let mut libs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("so" | "dylib" | "dll")))
        .collect();
    libs.sort();
    libs.into_iter().next()
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}
```

`registry.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use parking_lot::RwLock;
use tinyiothub_core::driver::manifest::DriverManifest;
use super::package::{LoadedPackage, hex_sha256, read_package};
use super::AccessError;

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub workspace_id: String,
    pub manifest: DriverManifest,
    pub manifest_hash: String,
    pub binary_path: Option<std::path::PathBuf>,
}

pub struct ManifestRegistry {
    active: RwLock<HashMap<(String, String), Arc<InstalledPackage>>>,
}

impl ManifestRegistry {
    pub fn new() -> Self { Self { active: RwLock::new(HashMap::new()) } }

    pub fn seed_builtin(&self, manifest_json: &str) -> Result<(), AccessError> {
        let manifest: DriverManifest = serde_json::from_str(manifest_json)?;
        tinyiothub_core::driver::manifest::validate_manifest(&manifest)?;
        let pkg = Arc::new(InstalledPackage {
            workspace_id: String::new(),
            manifest: manifest.clone(),
            manifest_hash: hex_sha256(manifest_json.as_bytes()),
            binary_path: None,
        });
        self.active.write().insert((String::new(), manifest.identity.driver), pkg);
        Ok(())
    }

    pub fn install(&self, workspace_id: &str, pkg: LoadedPackage) -> Result<InstalledPackage, AccessError> {
        let installed = InstalledPackage {
            workspace_id: workspace_id.to_string(),
            manifest: pkg.manifest,
            manifest_hash: pkg.manifest_hash,
            binary_path: pkg.binary_path,
        };
        self.active.write().insert(
            (workspace_id.to_string(), installed.manifest.identity.driver.clone()),
            Arc::new(installed.clone()),
        );
        Ok(installed)
    }

    pub fn install_dir(&self, workspace_id: &str, dir: &Path) -> Result<InstalledPackage, AccessError> {
        self.install(workspace_id, read_package(dir)?)
    }

    pub fn get(&self, workspace_id: &str, driver: &str) -> Option<Arc<InstalledPackage>> {
        let map = self.active.read();
        map.get(&(workspace_id.to_string(), driver.to_string())).cloned()
            .or_else(|| map.get(&(String::new(), driver.to_string())).cloned())
    }

    pub fn list(&self, workspace_id: &str) -> Vec<Arc<InstalledPackage>> {
        self.active.read().iter()
            .filter(|((ws, _), _)| ws == workspace_id || ws.is_empty())
            .map(|(_, v)| v.clone()).collect()
    }
}

impl Default for ManifestRegistry {
    fn default() -> Self { Self::new() }
}
```

Create empty `device.rs`, `materialize.rs`, `service.rs`, `validator.rs` files (zero bytes) so `mod.rs` compiles.

- [ ] **Step 5: Run tests**

Run: `cargo test -p runtime access::package`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/runtime/Cargo.toml crates/runtime/src/lib.rs crates/runtime/src/access/
git commit -m "feat(e4a): 驱动包读取与包级声明注册表"
```

---

### Task 6: 物化服务与投影写入

**Files:**
- Modify: `crates/runtime/src/access/materialize.rs`
- Test: inline fake store

**Interfaces:**
- Produces: `materialize_declaration(&dyn DeclarationProjectionStore, thing_id, &DeviceDeclaration)`

- [ ] **Step 1: Failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tinyiothub_core::driver::declaration::DeviceDeclaration;
    use tinyiothub_core::driver::manifest::*;

    #[derive(Default)]
    struct FakeStore {
        props: Mutex<Vec<(String, Vec<String>)>>,
        cmds: Mutex<Vec<(String, Vec<String>)>>,
        stale: Mutex<Vec<(String, Vec<String>, Vec<String>)>>,
    }

    #[async_trait::async_trait]
    impl DeclarationProjectionStore for FakeStore {
        async fn upsert_properties(&self, thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String> {
            self.props.lock().unwrap().push((thing_id.into(), fields.iter().map(|f| f.name.clone()).collect()));
            Ok(())
        }
        async fn upsert_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
            self.cmds.lock().unwrap().push((thing_id.into(), tools.iter().map(|t| t.name.clone()).collect()));
            Ok(())
        }
        async fn mark_stale(&self, thing_id: &str, keep_p: &[String], keep_c: &[String]) -> Result<(), String> {
            self.stale.lock().unwrap().push((thing_id.into(), keep_p.to_vec(), keep_c.to_vec()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn writes_properties_commands_and_stale_set() {
        let store = FakeStore::default();
        let decl = DeviceDeclaration {
            manifest_version: "1.0".into(), driver: "demo".into(), driver_version: "1.0.0".into(),
            state_fields: vec![StateFieldDecl {
                name: "level".into(), data_type: "integer".into(), unit: None,
                range: None, read_only: false, required: true, source: None,
            }],
            tools: vec![ManifestTool {
                name: "set_level".into(), kind: ToolKindDecl::Action, target_field: None,
                parameters: serde_json::json!({"type":"object"}), timeout: ToolTimeout::default(),
                safety: ToolSafetyDecl::Control, resources: vec![], cancellable: true,
            }],
        };
        materialize_declaration(&store, "t1", &decl).await.unwrap();
        assert_eq!(store.props.lock().unwrap()[0].1, vec!["level"]);
        assert_eq!(store.cmds.lock().unwrap()[0].1, vec!["set_level"]);
        assert_eq!(store.stale.lock().unwrap()[0].1, vec!["level"]);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p runtime access::materialize`
Expected: compile error.

- [ ] **Step 3: Implement**

```rust
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use crate::ports::DeclarationProjectionStore;

pub async fn materialize_declaration(
    store: &dyn DeclarationProjectionStore,
    thing_id: &str,
    decl: &DeviceDeclaration,
) -> Result<(), String> {
    store.upsert_properties(thing_id, &decl.state_fields).await?;
    store.upsert_commands(thing_id, &decl.tools).await?;
    let keep_properties: Vec<String> = decl.state_fields.iter().map(|f| f.name.clone()).collect();
    let keep_commands: Vec<String> = decl.tools.iter().map(|t| t.name.clone()).collect();
    store.mark_stale(thing_id, &keep_properties, &keep_commands).await
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p runtime access::materialize`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/runtime/src/access/materialize.rs
git commit -m "feat(e4a): 声明物化（属性/命令投影 + stale 标记）"
```

---

### Task 7: ThingDriver v2 签名、CancelToken、ABI v2 适配与 v1 拒载

**Files:**
- Modify: `crates/core/src/error.rs`, `crates/core/src/driver/mod.rs`
- Modify: `crates/runtime/src/driver/{wrapper,dynamic_adapter,registry,validator,mod}.rs`
- Modify all `impl ThingDriver for` sites:
  `crates/runtime/src/driver/drivers/{simulated_driver,modbus_driver,snmp_driver}.rs`,
  `crates/runtime/src/data_server.rs` (CountingDriver),
  `crates/runtime/tests/{push_driver_sink_test,derived_driver_test}.rs`,
  `apps/cloud/src/tests/park_derived_e2e_test.rs`
- Modify plugin-sdk doc examples: `crates/plugin-sdk/src/{lib.rs,macros.rs}`
- Test: inline + existing runtime tests

**Interfaces:**
- Produces: `CancelToken`, `Error::Cancelled`; trait methods
  `read_data(&mut self, &CancelToken)`, `execute_command(&mut self, &ThingCommand, &CancelToken)`,
  `set_demand(&mut self, &[String])`, `describe(&mut self) -> Result<DeviceDeclaration, Error>`;
  `DynamicThingDriver` ABI v2; registry v2-only

- [ ] **Step 1: Add CancelToken and Error::Cancelled**

`error.rs`: add `#[error("cancelled")] Cancelled,`.

`driver/mod.rs`:
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self { Self::default() }
    pub fn cancel(&self) { self.0.store(true, Ordering::SeqCst); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
}
```

Trait:
```rust
fn read_data(&mut self, cancel: &CancelToken) -> Result<Vec<ResultValue>, Error>;
fn execute_command(&mut self, cmd: &ThingCommand, cancel: &CancelToken) -> Result<bool, Error>;
fn set_demand(&mut self, _sources: &[String]) {}
fn describe(&mut self) -> Result<crate::driver::declaration::DeviceDeclaration, Error> {
    Err(Error::Unsupported("driver does not implement describe".into()))
}
```

- [ ] **Step 2: Find all call sites**

Run: `cargo check -p core -p runtime 2>&1 | head -100`
Expected: errors listing every impl/call site. Update the files listed above.

- [ ] **Step 3: Update DriverWrapper**

```rust
pub fn read_data_once(&mut self) -> DriverExecutionResult<Vec<ResultValue>> {
    self.read_data_once_with(&crate::driver::CancelToken::new())
}
pub fn read_data_once_with(&mut self, cancel: &crate::driver::CancelToken) -> DriverExecutionResult<Vec<ResultValue>> {
    let start_time = Instant::now();
    let result = self.retry_manager.execute_once(|| self.inner_driver.read_data(cancel));
    let elapsed = start_time.elapsed();
    self.update_status(&result, elapsed);
    self.convert_result(result, elapsed)
}
pub fn execute_command(&mut self, cmd: &ThingCommand) -> DriverExecutionResult<bool> {
    self.execute_command_with(cmd, &crate::driver::CancelToken::new())
}
pub fn execute_command_with(&mut self, cmd: &ThingCommand, cancel: &crate::driver::CancelToken) -> DriverExecutionResult<bool> {
    let start_time = Instant::now();
    let result = self.retry_manager.execute_once(|| self.inner_driver.execute_command(cmd, cancel));
    let elapsed = start_time.elapsed();
    self.update_status(&result, elapsed);
    self.convert_result(result, elapsed)
}
pub fn direct_read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
    self.inner_driver.read_data(&crate::driver::CancelToken::new())
}
pub fn direct_execute_command(&mut self, cmd: &ThingCommand) -> Result<bool, Error> {
    self.inner_driver.execute_command(cmd, &crate::driver::CancelToken::new())
}
```

`crates/runtime/src/driver/mod.rs`: `pub use tinyiothub_core::driver::{CancelToken, DeviceDeclaration, ThingDriver};`

- [ ] **Step 4: Implement ABI v2 dynamic adapter**

```rust
use std::sync::mpsc;
use std::time::Duration;
use tinyiothub_core::driver::CancelToken;
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::dynamic::{CancelHandle, DriverDone, DriverVTableV2};

struct DoneState {
    tx: mpsc::Sender<(i32, Option<String>)>,
    free: extern "C" fn(*mut c_char),
    /// Stable heap allocation the driver may poll from its worker thread.
    /// The callback reclaims it; on timeout it is intentionally leaked so
    /// the token stays valid for the still-running call (E4a staging).
    token: Box<CancelToken>,
}

extern "C" fn done_cb(user_data: *mut std::ffi::c_void, status: i32, out_json: *mut c_char) {
    // SAFETY: user_data is a Box<DoneState> allocated in `call`.
    let state = unsafe { Box::from_raw(user_data as *mut DoneState) };
    let json = if out_json.is_null() {
        None
    } else {
        let s = unsafe { std::ffi::CStr::from_ptr(out_json).to_string_lossy().into_owned() };
        (state.free)(out_json);
        Some(s)
    };
    let _ = state.tx.send((status, json));
}

extern "C" fn cancel_is_cancelled(ctx: *mut std::ffi::c_void) -> i32 {
    // SAFETY: ctx points into the Box<CancelToken> owned by DoneState; it is
    // valid until the callback reclaims DoneState (or leaks it on timeout).
    unsafe { (*(ctx as *const CancelToken)).is_cancelled() as i32 }
}

impl DynamicThingDriver {
    fn call(
        &self,
        invoke: impl FnOnce(*mut CancelHandle, DriverDone, *mut std::ffi::c_void) -> i32,
        cancel: &CancelToken,
    ) -> Result<String, Error> {
        let (tx, rx) = mpsc::channel();
        // The driver may poll the cancel handle from its worker thread after
        // this call returns (including after the staging timeout), so the
        // token must live at a stable address owned by DoneState.
        let token = Box::new(cancel.clone());
        let mut handle = CancelHandle {
            is_cancelled: cancel_is_cancelled,
            ctx: &*token as *const CancelToken as *mut std::ffi::c_void,
        };
        let state = Box::new(DoneState { tx, free: self.vtable.free_string, token });
        let user_data = Box::into_raw(state) as *mut std::ffi::c_void;
        let rc = invoke(&mut handle, done_cb, user_data);
        if rc != 0 {
            unsafe { drop(Box::from_raw(user_data as *mut DoneState)) };
            return Err(Error::DriverError(format!("driver call returned {rc}")));
        }
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok((0, Some(json))) => Ok(json),
            Ok((2, _)) => Err(Error::Cancelled),
            Ok((code, _)) => Err(Error::DriverError(format!("driver callback status {code}"))),
            Err(_) => Err(Error::DriverError("driver callback timed out".into())),
        }
    }
}
```

`read_data`/`execute_command` call `self.call` with the v2 function; `read_data` passes `"[]"` for sources (empty = all); `execute_command` serializes `ThingCommand`; `describe` calls `vtable.describe` and parses `DeviceDeclaration`; `set_demand` serializes source names and ignores the return.

- [ ] **Step 5: Switch registry/validator to v2**

`registry.rs`:
```rust
let vtable_fn: libloading::Symbol<unsafe extern "C" fn() -> *const DriverVTableV2> =
    match unsafe { lib.get(tinyiothub_core::driver::dynamic::SYMBOL_VTABLE_V2) } {
        Ok(f) => f,
        Err(_) => return Err(Error::DriverError(v1_unsupported_message(path))),
    };
let vtable_ptr = unsafe { vtable_fn() };
if vtable_ptr.is_null() { return Err(Error::DriverError("driver vtable v2 is null".into())); }
let vtable: &'static DriverVTableV2 = unsafe { &*vtable_ptr };
verify_v2_version(vtable.version).map_err(Error::DriverError)?;
```
Change `DynamicEntry.vtable` to `&'static DriverVTableV2`.

`DriverValidator::validate` loads `SYMBOL_VTABLE_V2`; missing → `InvalidFile(v1_unsupported_message(path))`.

- [ ] **Step 6: Update all impls**

For every `impl ThingDriver for`:
```rust
fn read_data(&mut self, cancel: &tinyiothub_core::driver::CancelToken) -> Result<Vec<ResultValue>, Error> {
    if cancel.is_cancelled() { return Err(Error::Cancelled); }
    // existing body
}
fn execute_command(&mut self, cmd: &ThingCommand, cancel: &tinyiothub_core::driver::CancelToken) -> Result<bool, Error> {
    if cancel.is_cancelled() { return Err(Error::Cancelled); }
    // existing body
}
```
Apply to Simulated/Modbus/Snmp (built-ins), `DynamicThingDriver` (ABI v2), CountingDriver, StubPushDriver, PollStubDriver, ManualPushDriver. Update plugin-sdk doc examples to the new signatures.

- [ ] **Step 7: Test and commit**

Run: `cargo test -p core -p runtime`
Expected: PASS.

```bash
git add crates/core/src/error.rs crates/core/src/driver/mod.rs crates/runtime/src/driver/ crates/runtime/src/data_server.rs crates/runtime/tests/ apps/cloud/src/tests/park_derived_e2e_test.rs crates/plugin-sdk/src/
git commit -m "refactor(e4a): ThingDriver v2 签名 + CancelToken + ABI v2 加载（v1 拒载）"
```

---

### Task 8: 内置驱动 manifest 与 describe

**Files:**
- Create: `crates/runtime/manifests/{simulator,modbus_rtu,snmp}.json`
- Create: `crates/runtime/src/driver/describe.rs`
- Modify: `crates/runtime/src/driver/{mod.rs,drivers/simulated_driver.rs,drivers/modbus_driver.rs,drivers/snmp_driver.rs}`
- Test: inline

**Interfaces:**
- Produces: `declaration_from_thing(&Thing, driver, version) -> DeviceDeclaration`, `builtin_manifest_jsons() -> [(&str,&str);3]`

- [ ] **Step 1: Manifests**

`simulator.json`:
```json
{
  "manifestVersion": "1.0",
  "identity": {
    "driver": "simulator", "version": "2.0.0", "protocol": "simulator",
    "displayName": "Simulated Thing Driver", "abiVersion": 2, "dynamicCapabilities": true
  },
  "config": { "type": "object", "additionalProperties": true },
  "capabilities": { "cancel": true, "push": false, "streaming": false },
  "tools": [], "stateFields": [],
  "sources": [{ "id": "telemetry", "kind": "poll", "fields": [], "intervalMs": 1000,
                "minIntervalMs": 200, "demandOnly": false, "health": true }],
  "resources": [], "placement": { "mode": "runtime" },
  "limits": { "maxConcurrentJobs": 1, "maxCallsPerMin": 60 }
}
```
`modbus_rtu.json`: same with `"driver":"modbus_rtu","version":"1.0.0","protocol":"modbus_rtu","displayName":"Modbus RTU/TCP Driver"`.
`snmp.json`: same with `"driver":"snmp","version":"1.0.0","protocol":"snmp","displayName":"SNMP Thing Driver (stub)"`.

- [ ] **Step 2: Failing tests**

`describe.rs` tests: build a `Thing` with one property (`temperature`, float, -40..125) and one command (`restart`); assert `declaration_from_thing` maps them.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::models::thing::{Thing, ThingStatus};
    use tinyiothub_core::models::thing_command::ThingCommand;
    use tinyiothub_core::models::thing_property::ThingProperty;

    fn thing() -> Thing {
        Thing {
            id: "t1".into(), name: "t1".into(), status: ThingStatus::Online,
            driver_name: Some("simulator".into()),
            properties: Some(vec![ThingProperty {
                id: "p1".into(), thing_id: "t1".into(), name: "temperature".into(),
                display_name: None, description: None, data_type: Some("float".into()),
                unit: Some("C".into()), min_value: Some(-40.0), max_value: Some(125.0),
                default_value: None, is_read_only: 1, source: Some("template".into()),
                expression: None, refs: None, created_at: None, updated_at: None,
                current_value: None, alarm_status: None, derived_status: None,
            }]),
            commands: Some(vec![ThingCommand {
                id: "c1".into(), thing_id: "t1".into(), name: "restart".into(),
                display_name: None, description: None,
                parameters: Some("{\"type\":\"object\"}".into()), created_at: "2026-10-21".into(),
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn maps_properties_to_state_fields() {
        let d = declaration_from_thing(&thing(), "simulator", "2.0.0");
        assert_eq!(d.state_fields[0].name, "temperature");
        assert_eq!(d.state_fields[0].range.as_ref().unwrap().max, Some(125.0));
    }

    #[test]
    fn maps_commands_to_action_tools() {
        let d = declaration_from_thing(&thing(), "simulator", "2.0.0");
        assert_eq!(d.tools[0].name, "restart");
        assert_eq!(d.tools[0].kind, tinyiothub_core::driver::manifest::ToolKindDecl::Action);
    }
}
```

- [ ] **Step 3: Implement**

```rust
//! Built-in driver instance declarations (E4a): derive from Thing config rows.

use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::manifest::{
    ManifestTool, NumericRange, StateFieldDecl, ToolKindDecl, ToolSafetyDecl,
};
use tinyiothub_core::models::thing::Thing;

pub fn declaration_from_thing(thing: &Thing, driver: &str, version: &str) -> DeviceDeclaration {
    let state_fields = thing.properties.as_deref().unwrap_or_default().iter().map(|p| StateFieldDecl {
        name: p.name.clone(),
        data_type: p.data_type.clone().unwrap_or_else(|| "string".into()),
        unit: p.unit.clone(),
        range: match (p.min_value, p.max_value) {
            (None, None) => None,
            (min, max) => Some(NumericRange { min, max }),
        },
        read_only: p.is_read_only != 0,
        required: true,
        source: Some("telemetry".into()),
    }).collect();
    let tools = thing.commands.as_deref().unwrap_or_default().iter().map(|c| ManifestTool {
        name: c.name.clone(),
        kind: ToolKindDecl::Action,
        target_field: None,
        parameters: c.parameters.as_deref().and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| serde_json::json!({"type":"object"})),
        timeout: Default::default(),
        safety: ToolSafetyDecl::Control,
        resources: vec![],
        cancellable: true,
    }).collect();
    DeviceDeclaration {
        manifest_version: "1.0".into(),
        driver: driver.into(),
        driver_version: version.into(),
        state_fields,
        tools,
    }
}

pub fn builtin_manifest_jsons() -> [(&'static str, &'static str); 3] {
    [
        ("simulator", include_str!("../../manifests/simulator.json")),
        ("modbus_rtu", include_str!("../../manifests/modbus_rtu.json")),
        ("snmp", include_str!("../../manifests/snmp.json")),
    ]
}
```

- [ ] **Step 4: Implement describe on the three drivers**

```rust
fn describe(&mut self) -> Result<tinyiothub_core::driver::declaration::DeviceDeclaration, Error> {
    Ok(crate::driver::describe::declaration_from_thing(&self.device, "simulator", "2.0.0"))
}
```
Use `"modbus_rtu"`/`"1.0.0"` and `"snmp"`/`"1.0.0"` in the other drivers.

Add `pub mod describe;` to `driver/mod.rs`.

- [ ] **Step 5: Test and commit**

Run: `cargo test -p runtime driver::describe`
Expected: PASS.

```bash
git add crates/runtime/manifests/ crates/runtime/src/driver/describe.rs crates/runtime/src/driver/mod.rs crates/runtime/src/driver/drivers/
git commit -m "feat(e4a): 内置驱动 manifest 与实例声明 describe"
```

---

### Task 9: demo-declarative 参考驱动

**Files:**
- Modify: `Cargo.toml` members
- Create: `drivers/demo-declarative/{Cargo.toml,driver.manifest.json,src/lib.rs}`
- Test: inline

**Interfaces:**
- Produces: cdylib exporting `tinyiothub_driver_init`, `tinyiothub_driver_vtable_v2`, `tinyiothub_driver_destroy`

- [ ] **Step 1: Workspace member and crate**

Root `Cargo.toml` members add `"drivers/demo-declarative",`.

`Cargo.toml`:
```toml
[package]
name = "demo-declarative"
version = "1.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]
name = "demo_declarative"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
core = { path = "../../crates/core" }
```

- [ ] **Step 2: Manifest**

`driver.manifest.json` (exact content from spec §2.1 example, plus `long_task`):

```json
{
  "manifestVersion": "1.0",
  "identity": { "driver": "demo-declarative", "version": "1.0.0", "protocol": "demo",
                "displayName": "Demo Declarative Driver", "abiVersion": 2,
                "dynamicCapabilities": false },
  "config": { "type": "object",
              "properties": { "intervalMs": { "type": "integer", "minimum": 100, "maximum": 60000, "default": 1000 } },
              "additionalProperties": false },
  "capabilities": { "cancel": true, "push": false, "streaming": false },
  "tools": [
    { "name": "echo", "kind": "action",
      "parameters": { "type": "object", "properties": { "message": { "type": "string" } },
                      "required": ["message"], "additionalProperties": false },
      "timeout": { "defaultMs": 1000, "maxMs": 2000 },
      "safety": "safe", "resources": [], "cancellable": true },
    { "name": "set_level", "kind": "set", "targetField": "level",
      "parameters": { "type": "object", "properties": { "level": { "type": "integer", "minimum": 0, "maximum": 100 } },
                      "required": ["level"], "additionalProperties": false },
      "timeout": { "defaultMs": 3000, "maxMs": 5000 },
      "safety": "control", "resources": ["actuator"], "cancellable": true },
    { "name": "long_task", "kind": "action",
      "parameters": { "type": "object", "properties": { "ms": { "type": "integer", "minimum": 100, "maximum": 10000 } },
                      "required": ["ms"], "additionalProperties": false },
      "timeout": { "defaultMs": 10000, "maxMs": 10000 },
      "safety": "safe", "resources": ["actuator"], "cancellable": true }
  ],
  "stateFields": [{ "name": "level", "type": "integer", "unit": "%",
                    "range": { "min": 0, "max": 100 }, "readOnly": false,
                    "required": true, "source": "telemetry" }],
  "sources": [{ "id": "telemetry", "kind": "poll", "fields": ["level"],
                "intervalMs": 1000, "minIntervalMs": 200, "demandOnly": false, "health": true }],
  "resources": [{ "id": "actuator", "mode": "exclusive", "capacity": 1 }],
  "placement": { "mode": "runtime", "fromSource": "position", "trust": "trusted" },
  "limits": { "maxConcurrentJobs": 1, "maxCallsPerMin": 60 }
}
```

- [ ] **Step 3: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_valid() {
        let m: tinyiothub_core::driver::manifest::DriverManifest =
            serde_json::from_str(include_str!("../driver.manifest.json")).unwrap();
        tinyiothub_core::driver::manifest::validate_manifest(&m).unwrap();
    }

    #[test]
    fn set_level_updates_state_and_rejects_out_of_range() {
        let mut s = DemoState::default();
        assert_eq!(s.level, 0);
        s.set_level(42).unwrap();
        assert_eq!(s.level, 42);
        assert!(s.set_level(101).is_err());
    }

    #[test]
    fn long_task_aborts_when_cancelled() {
        let flag = Arc::new(AtomicBool::new(false));
        let f = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            f.store(true, Ordering::SeqCst);
        });
        let check = flag.clone();
        let started = std::time::Instant::now();
        assert!(run_long_task(1000, move || check.load(Ordering::SeqCst)).is_err());
        assert!(started.elapsed() < std::time::Duration::from_millis(250));
    }
}
```

- [ ] **Step 4: Run to verify failure**

Run: `cargo test -p demo-declarative`
Expected: compile error.

- [ ] **Step 5: Implement the cdylib**

```rust
//! Reference declarative driver (E4a). Build: cargo build -p demo-declarative

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tinyiothub_core::driver::dynamic::{
    CancelHandle, DriverDone, DriverVTableV2, DRIVER_ABI_VERSION_V2,
};
use tinyiothub_core::driver::manifest::DriverManifest;

#[derive(Default)]
struct DemoState { level: i64 }

impl DemoState {
    fn set_level(&mut self, level: i64) -> Result<(), String> {
        if !(0..=100).contains(&level) {
            return Err(format!("level out of range: {level}"));
        }
        self.level = level;
        Ok(())
    }
}

fn run_long_task(ms: u64, is_cancelled: impl Fn() -> bool) -> Result<(), String> {
    let step = 50u64;
    let mut elapsed = 0u64;
    while elapsed < ms {
        if is_cancelled() {
            return Err("cancelled".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(step.min(ms - elapsed)));
        elapsed += step;
    }
    Ok(())
}

fn emit(done: DriverDone, user: *mut c_void, status: c_int, json: String) {
    let c = CString::new(json).unwrap_or_default();
    done(user, status, c.into_raw());
}

fn param(v: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
    let p = v.get("parameters")?;
    if let Some(s) = p.as_str() {
        serde_json::from_str::<serde_json::Value>(s).ok()?.get(key).cloned()
    } else {
        p.get(key).cloned()
    }
}

extern "C" fn demo_read(
    ctx: *mut c_void, _config: *const c_char, _sources: *const c_char,
    _cancel: *mut CancelHandle, done: DriverDone, user: *mut c_void,
) -> c_int {
    let state = unsafe { &*(ctx as *const DemoState) };
    emit(done, user, 0, format!(r#"[{{"name":"level","value_type":"int","value":"{}"}}]"#, state.level));
    0
}

extern "C" fn demo_execute(
    ctx: *mut c_void, _config: *const c_char, cmd_json: *const c_char,
    cancel: *mut CancelHandle, done: DriverDone, user: *mut c_void,
) -> c_int {
    let state = unsafe { &mut *(ctx as *mut DemoState) };
    let raw = unsafe { CStr::from_ptr(cmd_json) }.to_string_lossy().to_string();
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => { emit(done, user, 1, serde_json::json!({"error": e.to_string()}).to_string()); return 0; }
    };
    match v.get("name").and_then(|x| x.as_str()).unwrap_or("") {
        "echo" => {
            let msg = param(&v, "message").and_then(|x| x.as_str().map(str::to_string)).unwrap_or_default();
            emit(done, user, 0, "true".to_string());
        }
        "set_level" => {
            let level = param(&v, "level").and_then(|x| x.as_i64()).unwrap_or(-1);
            match state.set_level(level) {
                Ok(()) => emit(done, user, 0, "true".to_string()),
                Err(e) => emit(done, user, 1, serde_json::json!({"error": e}).to_string()),
            }
        }
        "long_task" => {
            let ms = param(&v, "ms").and_then(|x| x.as_u64()).unwrap_or(1000);
            let done_addr = done as usize;
            let user_addr = user as usize;
            let cancel_fn: Option<(usize, usize)> = if cancel.is_null() {
                None
            } else {
                unsafe { Some(((*cancel).is_cancelled as usize, (*cancel).ctx as usize)) }
            };
            std::thread::spawn(move || {
                let done_f: DriverDone = unsafe { std::mem::transmute(done_addr) };
                let user_p = user_addr as *mut c_void;
                let is_cancelled = move || match cancel_fn {
                    Some((f, ctx)) => unsafe {
                        let f: extern "C" fn(*mut c_void) -> c_int = std::mem::transmute(f);
                        f(ctx as *mut c_void) != 0
                    },
                    None => false,
                };
                match run_long_task(ms, is_cancelled) {
                    Ok(()) => emit(done_f, user_p, 0, "true".to_string()),
                    Err(e) => emit(done_f, user_p, 2, serde_json::json!({"error": e}).to_string()),
                }
            });
        }
        other => emit(done, user, 1, serde_json::json!({"error": format!("unknown tool {other}")}).to_string()),
    }
    0
}

extern "C" fn demo_set_demand(_ctx: *mut c_void, _demand: *const c_char) -> c_int { 0 }

extern "C" fn demo_describe(
    _ctx: *mut c_void, _config: *const c_char, out: *mut *mut c_char,
) -> c_int {
    let m: DriverManifest = serde_json::from_str(include_str!("../driver.manifest.json")).unwrap();
    let decl = tinyiothub_core::driver::declaration::DeviceDeclaration {
        manifest_version: m.manifest_version.clone(),
        driver: m.identity.driver.clone(),
        driver_version: m.identity.version.clone(),
        state_fields: m.state_fields.clone(),
        tools: m.tools.clone(),
    };
    if out.is_null() { return 1; }
    let s = CString::new(serde_json::to_string(&decl).unwrap()).unwrap();
    unsafe { *out = s.into_raw() };
    0
}

extern "C" fn demo_free_string(s: *mut c_char) {
    if !s.is_null() { unsafe { drop(CString::from_raw(s)) }; }
}

static VTABLE: DriverVTableV2 = DriverVTableV2 {
    version: DRIVER_ABI_VERSION_V2,
    read_async: demo_read,
    execute_async: demo_execute,
    set_demand: demo_set_demand,
    describe: demo_describe,
    free_string: demo_free_string,
};

#[no_mangle]
pub extern "C" fn tinyiothub_driver_init() -> *mut c_void {
    Box::into_raw(Box::new(DemoState::default())) as *mut c_void
}

#[no_mangle]
pub extern "C" fn tinyiothub_driver_vtable_v2() -> *const DriverVTableV2 {
    &VTABLE
}

#[no_mangle]
pub extern "C" fn tinyiothub_driver_destroy(ctx: *mut c_void) {
    if !ctx.is_null() {
        unsafe { drop(Box::from_raw(ctx as *mut DemoState)) };
    }
}
```

The worker polls the ABI cancel handle every 50 ms and emits status 2 on cancellation; `stop_device` wiring is E4c. Execute status 0 is a JSON boolean (`true`) because the hub parses `execute_command` results as `bool`.

- [ ] **Step 6: Test and build**

Run: `cargo test -p demo-declarative`
Expected: PASS (5 tests).
Run: `cargo build -p demo-declarative`
Expected: `target/debug/libdemo_declarative.{so,dylib}` exists.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml drivers/demo-declarative/
git commit -m "feat(e4a): demo-declarative 参考驱动（manifest + ABI v2 cdylib）"
```

---

### Task 10: AccessService、安装 API 与 ThingService 物化接线

**Files:**
- Modify: `crates/runtime/src/access/{device.rs,service.rs}`
- Create: `apps/cloud/src/shared/access_ports.rs`
- Create: `apps/cloud/src/domains/driver/declaration/{mod.rs,handler.rs}`
- Modify: `apps/cloud/src/domains/driver/mod.rs`
- Modify: `apps/cloud/src/domains/thing/hooks.rs`
- Modify: `apps/cloud/src/domains/thing/service/mod.rs`
- Modify: `apps/cloud/src/domains/thing/handler/crud.rs`
- Modify: `apps/cloud/src/state.rs`, `apps/cloud/src/shared/service_manager.rs`
- Create: `apps/cloud/src/tests/access_declaration_api_test.rs`
- Modify: `apps/cloud/src/tests/mod.rs`

**Interfaces:**
- Produces: `AccessService::{new, install_dir, get_manifest, package_dir, bind_thing, load_declaration}`; `ThingDeclarationHook`; AppState `access`; routes `POST /v1/drivers/install`, `GET /v1/drivers/declarations`, `GET /v1/drivers/{driver}/declaration`

- [ ] **Step 1: DeviceDeclarationRegistry**

`crates/runtime/src/access/device.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tinyiothub_core::driver::declaration::DeviceDeclaration;

#[derive(Default)]
pub struct DeviceDeclarationRegistry {
    by_thing: RwLock<HashMap<(String, String), Arc<DeviceDeclaration>>>,
}

impl DeviceDeclarationRegistry {
    pub fn upsert(&self, workspace_id: &str, thing_id: &str, decl: DeviceDeclaration) {
        self.by_thing.write().insert((workspace_id.into(), thing_id.into()), Arc::new(decl));
    }
    pub fn get(&self, workspace_id: &str, thing_id: &str) -> Option<Arc<DeviceDeclaration>> {
        self.by_thing.read().get(&(workspace_id.into(), thing_id.into())).cloned()
    }
    pub fn remove(&self, workspace_id: &str, thing_id: &str) {
        self.by_thing.write().remove(&(workspace_id.into(), thing_id.into()));
    }
}
```

- [ ] **Step 2: AccessService**

First add an explicit-name loader to `crates/runtime/src/driver/registry.rs` (the current `load` derives the name from the file stem, which breaks manifest-named packages):

```rust
pub fn load(&self, path: &PathBuf, workspace_id: &str) -> Result<String, Error> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.strip_prefix("lib").unwrap_or(s))
        .unwrap_or("unknown")
        .to_string();
    self.load_named(path, workspace_id, &name)
}

pub fn load_named(&self, path: &PathBuf, workspace_id: &str, driver_name: &str) -> Result<String, Error> {
    // Move the existing body of `load` here, replacing the derived
    // `driver_name` variable with the parameter. Keep the duplicate-name
    // check and ref_count handling unchanged.
}
```

Then create `crates/runtime/src/access/service.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tinyiothub_core::driver::declaration::{
    DeviceDeclaration, DeviceDeclarationRecord, DriverDeclarationRecord, fixed_declaration,
    validate_device_declaration,
};
use tinyiothub_core::driver::manifest::MANIFEST_FILE;
use tinyiothub_core::models::thing::Thing;

use super::device::DeviceDeclarationRegistry;
use super::materialize::materialize_declaration;
use super::package::{hex_sha256, read_package};
use super::registry::{InstalledPackage, ManifestRegistry};
use super::AccessError;
use crate::ports::{
    AcceptanceReportStore, DeclarationProjectionStore, DeviceDeclarationStore, DriverDeclarationStore,
    ThingDeclarationSource,
};

pub struct AccessService {
    pub data_dir: PathBuf,
    pub manifests: Arc<ManifestRegistry>,
    pub devices: Arc<DeviceDeclarationRegistry>,
    pub driver_store: Arc<dyn DriverDeclarationStore>,
    pub device_store: Arc<dyn DeviceDeclarationStore>,
    pub projection: Arc<dyn DeclarationProjectionStore>,
    pub things: Arc<dyn ThingDeclarationSource>,
    pub acceptance_store: Arc<dyn AcceptanceReportStore>,
}

impl AccessService {
    pub fn new(
        data_dir: PathBuf,
        manifests: Arc<ManifestRegistry>,
        driver_store: Arc<dyn DriverDeclarationStore>,
        device_store: Arc<dyn DeviceDeclarationStore>,
        projection: Arc<dyn DeclarationProjectionStore>,
        things: Arc<dyn ThingDeclarationSource>,
        acceptance_store: Arc<dyn AcceptanceReportStore>,
    ) -> Self {
        Self {
            data_dir,
            manifests,
            devices: Arc::new(DeviceDeclarationRegistry::default()),
            driver_store,
            device_store,
            projection,
            things,
            acceptance_store,
        }
    }

    pub async fn install_dir(&self, workspace_id: &str, src: &Path) -> Result<InstalledPackage, AccessError> {
        let src_pkg = read_package(src)?;
        let id = &src_pkg.manifest.identity;
        let dest = self
            .data_dir
            .join("drivers/workspaces")
            .join(workspace_id)
            .join(&id.driver)
            .join(&id.version);
        std::fs::create_dir_all(&dest)?;
        std::fs::write(dest.join(MANIFEST_FILE), &src_pkg.manifest_json)?;
        if let Some(bin) = &src_pkg.binary_path {
            let ext = bin.extension().and_then(|e| e.to_str()).unwrap_or("so");
            std::fs::copy(bin, dest.join(format!("driver.{ext}")))?;
        }
        let pkg = read_package(&dest)?;
        if let Some(bin) = &pkg.binary_path {
            let registry = crate::driver::driver_registry();
            let mut guard = registry.write();
            if guard.find(workspace_id, &pkg.manifest.identity.driver).is_some() {
                guard
                    .unload(&pkg.manifest.identity.driver, workspace_id)
                    .map_err(|e| AccessError::Abi(e.to_string()))?;
            }
            guard
                .load_named(bin, workspace_id, &pkg.manifest.identity.driver)
                .map_err(|e| AccessError::Abi(e.to_string()))?;
        }
        let now = Utc::now().to_rfc3339();
        let rec = DriverDeclarationRecord {
            workspace_id: workspace_id.into(),
            driver_name: pkg.manifest.identity.driver.clone(),
            driver_version: pkg.manifest.identity.version.clone(),
            manifest_version: pkg.manifest.manifest_version.clone(),
            manifest_json: pkg.manifest_json.clone(),
            manifest_hash: pkg.manifest_hash.clone(),
            binary_hash: pkg.binary_hash.clone(),
            status: "active".into(),
            installed_at: now,
        };
        self.driver_store.upsert_active(&rec).await.map_err(AccessError::Store)?;
        self.manifests.install(workspace_id, pkg)
    }

    pub fn get_manifest(&self, workspace_id: &str, driver: &str) -> Option<Arc<InstalledPackage>> {
        self.manifests.get(workspace_id, driver)
    }

    pub fn package_dir(&self, workspace_id: &str, driver: &str) -> Option<PathBuf> {
        let pkg = self.manifests.get(workspace_id, driver)?;
        let id = &pkg.manifest.identity;
        Some(
            self.data_dir
                .join("drivers/workspaces")
                .join(workspace_id)
                .join(&id.driver)
                .join(&id.version),
        )
    }

    pub async fn bind_thing(&self, workspace_id: &str, thing: &Thing) -> Result<DeviceDeclaration, AccessError> {
        let driver_name = thing
            .driver_name
            .clone()
            .ok_or_else(|| AccessError::NotFound("<none>".into(), workspace_id.into()))?;
        let pkg = self
            .manifests
            .get(workspace_id, &driver_name)
            .ok_or_else(|| AccessError::NotFound(driver_name.clone(), workspace_id.into()))?;
        let decl = if pkg.manifest.identity.dynamic_capabilities {
            let mut driver = crate::driver::create_driver(&driver_name, thing)
                .map_err(|e| AccessError::Abi(e.to_string()))?;
            driver.inner_driver_mut().describe().map_err(|e| AccessError::Abi(e.to_string()))?
        } else {
            fixed_declaration(&pkg.manifest)
        };
        validate_device_declaration(&pkg.manifest, &decl)?;
        let json = serde_json::to_string(&decl)?;
        let rec = DeviceDeclarationRecord {
            workspace_id: workspace_id.into(),
            thing_id: thing.id.clone(),
            driver_name: driver_name.clone(),
            driver_version: decl.driver_version.clone(),
            declaration_json: json.clone(),
            declaration_hash: hex_sha256(json.as_bytes()),
            status: "active".into(),
            updated_at: Utc::now().to_rfc3339(),
        };
        self.device_store.upsert(&rec).await.map_err(AccessError::Store)?;
        materialize_declaration(self.projection.as_ref(), &thing.id, &decl)
            .await
            .map_err(AccessError::Store)?;
        self.devices.upsert(workspace_id, &thing.id, decl.clone());
        Ok(decl)
    }

    pub async fn load_declaration(
        &self,
        workspace_id: &str,
        thing_id: &str,
    ) -> Result<Option<DeviceDeclaration>, AccessError> {
        if let Some(d) = self.devices.get(workspace_id, thing_id) {
            return Ok(Some((*d).clone()));
        }
        let Some(row) = self.device_store.find(workspace_id, thing_id).await.map_err(AccessError::Store)? else {
            return Ok(None);
        };
        let decl: DeviceDeclaration = serde_json::from_str(&row.declaration_json)?;
        self.devices.upsert(workspace_id, thing_id, decl.clone());
        Ok(Some(decl))
    }
}
```

`service.rs` uses `crate::driver::create_driver`; ensure it is public (it is).

- [ ] **Step 3: Cloud port adapters**

`apps/cloud/src/shared/access_ports.rs`:

```rust
use std::sync::Arc;
use async_trait::async_trait;
use tinyiothub_storage::Db;
use tinyiothub_core::driver::declaration::{
    AcceptanceReportRecord, DeviceDeclarationRecord, DriverDeclarationRecord,
};
use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_runtime::ports::*;

pub struct DbDriverStore(pub Arc<Db>);
#[async_trait]
impl DriverDeclarationStore for DbDriverStore {
    async fn upsert_active(&self, r: &DriverDeclarationRecord) -> Result<(), String> {
        self.0.upsert_driver_declaration(r).await.map_err(|e| e.to_string())
    }
    async fn list_active(&self, ws: &str) -> Result<Vec<DriverDeclarationRecord>, String> {
        self.0.list_active_driver_declarations(ws).await.map_err(|e| e.to_string())
    }
    async fn list_all(&self) -> Result<Vec<DriverDeclarationRecord>, String> {
        self.0.list_all_driver_declarations().await.map_err(|e| e.to_string())
    }
}

pub struct DbDeviceStore(pub Arc<Db>);
#[async_trait]
impl DeviceDeclarationStore for DbDeviceStore {
    async fn upsert(&self, r: &DeviceDeclarationRecord) -> Result<(), String> {
        self.0.upsert_device_declaration(r).await.map_err(|e| e.to_string())
    }
    async fn list_by_workspace(&self, ws: &str) -> Result<Vec<DeviceDeclarationRecord>, String> {
        self.0.list_device_declarations(ws).await.map_err(|e| e.to_string())
    }
    async fn find(&self, ws: &str, thing_id: &str) -> Result<Option<DeviceDeclarationRecord>, String> {
        self.0.find_device_declaration(ws, thing_id).await.map_err(|e| e.to_string())
    }
}

pub struct DbProjectionStore(pub Arc<Db>);
#[async_trait]
impl DeclarationProjectionStore for DbProjectionStore {
    async fn upsert_properties(&self, thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String> {
        self.0.upsert_declared_properties(thing_id, fields).await.map_err(|e| e.to_string())
    }
    async fn upsert_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
        self.0.upsert_declared_commands(thing_id, tools).await.map_err(|e| e.to_string())
    }
    async fn mark_stale(&self, thing_id: &str, keep_p: &[String], keep_c: &[String]) -> Result<(), String> {
        self.0.mark_stale_declared_capabilities(thing_id, keep_p, keep_c)
            .await.map_err(|e| e.to_string())
    }
}

pub struct DbThingSource(pub Arc<Db>);
#[async_trait]
impl ThingDeclarationSource for DbThingSource {
    async fn load_thing(&self, thing_id: &str) -> Result<Option<Thing>, String> {
        self.0.find_thing_with_properties(thing_id).await.map_err(|e| e.to_string())
    }
}

pub struct DbAcceptanceStore(pub Arc<Db>);
#[async_trait]
impl AcceptanceReportStore for DbAcceptanceStore {
    async fn insert(&self, r: &AcceptanceReportRecord) -> Result<(), String> {
        self.0.insert_acceptance_report(r).await.map_err(|e| e.to_string())
    }
}
```

Add `pub mod access_ports;` to `apps/cloud/src/shared/mod.rs`.

- [ ] **Step 4: ThingDeclarationHook and ThingService**

`apps/cloud/src/domains/thing/hooks.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationMode {
    Declarative,
    TemplateBootstrap,
}

#[async_trait::async_trait]
pub trait ThingDeclarationHook: Send + Sync {
    async fn prepare_creation(
        &self,
        workspace_id: &str,
        driver_name: &str,
        options: Option<&str>,
    ) -> Result<CreationMode, String>;
    async fn materialize(&self, workspace_id: &str, thing_id: &str) -> Result<(), String>;
}
```

`ThingService`:
```rust
declaration_hook: parking_lot::RwLock<Option<Arc<dyn super::hooks::ThingDeclarationHook>>>,
```
Initialize with `parking_lot::RwLock::new(None)` in `new`. Add:
```rust
pub fn set_declaration_hook(&self, hook: Arc<dyn super::hooks::ThingDeclarationHook>) {
    *self.declaration_hook.write() = Some(hook);
}
```

In `create_thing`, before insert:
```rust
let hook = self.declaration_hook.read().clone();
let mut creation_mode = None;
if let (Some(hook), Some(driver)) = (&hook, &req.driver_name) {
    creation_mode = Some(
        hook.prepare_creation(workspace_id.unwrap_or_default(), driver, req.driver_options.as_deref())
            .await
            .map_err(ThingError::Validation)?,
    );
}
```
After insert:
```rust
if let Some(ref tid) = req.template_id {
    let bootstrap = !matches!(creation_mode, Some(CreationMode::Declarative));
    if bootstrap {
        let _ = self.copy_template_props(&created.id, tid).await;
        let _ = self.copy_template_acts(&created.id, tid).await;
    }
}
if let (Some(hook), Some(_)) = (&hook, &req.driver_name) {
    hook.materialize(workspace_id.unwrap_or_default(), &created.id)
        .await
        .map_err(ThingError::Validation)?;
}
```
Import `CreationMode` from `super::hooks`.

- [ ] **Step 5: Hook adapter and handler**

`apps/cloud/src/domains/driver/declaration/mod.rs`:

```rust
pub mod handler;

use std::sync::Arc;
use async_trait::async_trait;
use tinyiothub_core::driver::schema::validate_params;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_runtime::access::service::AccessService;
use crate::domains::thing::hooks::{CreationMode, ThingDeclarationHook};

pub struct AccessDeclarationHook {
    pub access: Arc<AccessService>,
}

#[async_trait]
impl ThingDeclarationHook for AccessDeclarationHook {
    async fn prepare_creation(
        &self, workspace_id: &str, driver_name: &str, options: Option<&str>,
    ) -> Result<CreationMode, String> {
        let pkg = self
            .access
            .get_manifest(workspace_id, driver_name)
            .ok_or_else(|| format!("driver '{driver_name}' has no installed manifest"))?;
        let opts: serde_json::Value = match options {
            Some(raw) => serde_json::from_str(raw)
                .map_err(|e| format!("invalid driver_options JSON: {e}"))?,
            None => serde_json::json!({}),
        };
        validate_params(&pkg.manifest.config, &opts)
            .map_err(|e| format!("driver_options invalid: {e}"))?;
        Ok(if pkg.manifest.identity.dynamic_capabilities {
            CreationMode::TemplateBootstrap
        } else {
            CreationMode::Declarative
        })
    }

    async fn materialize(&self, workspace_id: &str, thing_id: &str) -> Result<(), String> {
        let thing: Thing = self
            .access
            .things
            .load_thing(thing_id)
            .await?
            .ok_or_else(|| format!("thing '{thing_id}' not found"))?;
        self.access
            .bind_thing(workspace_id, &thing)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
```

`handler.rs` (routes are merged into the existing driver router; `WorkspaceScope` supplies ws):

```rust
use axum::extract::{Multipart, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tinyiothub_web::middleware::workspace::WorkspaceScope;
use crate::state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/install", post(install_driver))
        .route("/declarations", get(list_declarations))
        .route("/{driver}/declaration", get(get_declaration))
}

async fn install_driver(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    mut multipart: Multipart,
) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => return Json(json!({"success": false, "error": e.to_string()})),
    };
    let mut manifest_seen = false;
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("manifest") => match field.text().await {
                Ok(text) => {
                    if std::fs::write(dir.path().join("driver.manifest.json"), text).is_err() {
                        return Json(json!({"success": false, "error": "write manifest failed"}));
                    }
                    manifest_seen = true;
                }
                Err(e) => return Json(json!({"success": false, "error": e.to_string()})),
            },
            Some("binary") => match field.bytes().await {
                Ok(bytes) => {
                    if std::fs::write(dir.path().join("driver.so"), bytes).is_err() {
                        return Json(json!({"success": false, "error": "write binary failed"}));
                    }
                }
                Err(e) => return Json(json!({"success": false, "error": e.to_string()})),
            },
            _ => {}
        }
    }
    if !manifest_seen {
        return Json(json!({"success": false, "error": "missing manifest field"}));
    }
    match access.install_dir(&ws, dir.path()).await {
        Ok(pkg) => Json(json!({"success": true, "driver": pkg.manifest.identity.driver,
                               "version": pkg.manifest.identity.version,
                               "manifestHash": pkg.manifest_hash})),
        Err(e) => Json(json!({"success": false, "error": e.to_string()})),
    }
}

async fn list_declarations(State(state): State<AppState>, WorkspaceScope(ws): WorkspaceScope) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    let items: Vec<Value> = access.manifests.list(&ws).into_iter().map(|p| json!({
        "driver": p.manifest.identity.driver,
        "version": p.manifest.identity.version,
        "protocol": p.manifest.identity.protocol,
        "manifestHash": p.manifest_hash,
        "dynamicCapabilities": p.manifest.identity.dynamic_capabilities
    })).collect();
    Json(json!({"success": true, "data": items}))
}

async fn get_declaration(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(driver): Path<String>,
) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    match access.get_manifest(&ws, &driver) {
        Some(pkg) => Json(json!({"success": true, "data": pkg.manifest})),
        None => Json(json!({"success": false, "error": "driver not installed"})),
    }
}
```

`apps/cloud/src/domains/driver/mod.rs`: change `router` to `Router<AppState>` and merge:
```rust
pub fn router() -> axum::Router<crate::state::AppState> {
    drivers::handler::create_router().merge(declaration::handler::create_router())
}
```
Add `pub mod declaration;`.

- [ ] **Step 6: AppState and ServiceManager**

`state.rs`: add `pub access: Option<Arc<tinyiothub_runtime::access::service::AccessService>>,` init `None`; add `set_access`.

`service_manager.rs` after DataServer:
```rust
let manifests = Arc::new(tinyiothub_runtime::access::registry::ManifestRegistry::new());
for (_, json) in tinyiothub_runtime::driver::describe::builtin_manifest_jsons() {
    manifests.seed_builtin(json)?;
}
let access = Arc::new(tinyiothub_runtime::access::service::AccessService::new(
    std::path::PathBuf::from("data"),
    manifests,
    Arc::new(DbDriverStore(app_state.db.clone())),
    Arc::new(DbDeviceStore(app_state.db.clone())),
    Arc::new(DbProjectionStore(app_state.db.clone())),
    Arc::new(DbThingSource(app_state.db.clone())),
    Arc::new(DbAcceptanceStore(app_state.db.clone())),
));
app_state.set_access(access);
```
`ServiceManager::start_all` already receives `&mut AppState`, so `app_state.set_access` is valid. Add `tempfile = { workspace = true }` to `apps/cloud/Cargo.toml` `[dependencies]` (it is currently dev-only, but the multipart handler is production code).

- [ ] **Step 7: Attach hook to per-request ThingService**

In `apps/cloud/src/domains/thing/handler/crud.rs`:
```rust
fn thing_service(state: &AppState) -> ThingService {
    let svc = ThingService::new(state.db.pool().clone());
    if let Some(access) = &state.access {
        svc.set_declaration_hook(Arc::new(
            crate::domains::driver::declaration::AccessDeclarationHook { access: access.clone() },
        ));
    }
    svc
}
```
Replace every `thing_service(&pool)` / `let pool = ...; thing_service(&pool)` in `crud.rs` with `thing_service(&state)` and remove the now-unused `pool` bindings (or keep where used elsewhere).

- [ ] **Step 8: Handler test**

`apps/cloud/src/tests/access_declaration_api_test.rs`: reuse the AppState/router setup pattern from `driver_handler_tests.rs`; assert:
1. multipart install with the demo manifest returns `success: true`;
2. `GET /v1/drivers/declarations` contains `demo-declarative`;
3. reinstalling the same manifest keeps one entry.

Add `mod access_declaration_api_test;` to `tests/mod.rs`.

- [ ] **Step 9: Test and commit**

Run: `cargo test -p tinyiothub-cloud access_declaration`
Expected: PASS.

```bash
git add crates/runtime/src/access/ apps/cloud/src/shared/access_ports.rs apps/cloud/src/shared/mod.rs apps/cloud/src/domains/driver/declaration/ apps/cloud/src/domains/driver/mod.rs apps/cloud/src/domains/thing/hooks.rs apps/cloud/src/domains/thing/service/mod.rs apps/cloud/src/domains/thing/handler/crud.rs apps/cloud/src/state.rs apps/cloud/src/shared/service_manager.rs apps/cloud/src/tests/
git commit -m "feat(e4a): AccessService、安装 API 与建物物化接线"
```

---

### Task 11: 调用边界校验接线（call_device / invoke_action）

**Files:**
- Modify: `crates/runtime/src/access/validator.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/invoke_action.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/thing/mod.rs`
- Modify: `apps/cloud/src/domains/agent/host/autonomous_factory.rs`
- Modify: `apps/cloud/src/domains/agent/host/tools/autonomous_invoke.rs`
- Test: `crates/runtime/src/access/validator.rs` unit tests + `apps/cloud/src/tests/invoke_action_flow_tests.rs`

**Interfaces:**
- Produces: `ValidatedCall`, `CallError`, `AccessCallValidator` trait; `InvokeActionTool.access: Option<Arc<dyn AccessCallValidator>>`

- [ ] **Step 1: Validator + trait**

`validator.rs`:

```rust
use async_trait::async_trait;
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::manifest::{ManifestTool, validate_params};

#[derive(Debug, Clone)]
pub struct ValidatedCall {
    pub tool: ManifestTool,
    pub timeout_ms: u64,
    pub clamped: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("no declaration for thing '{0}'")]
    NoDeclaration(String),
    #[error("action '{0}' not declared")]
    UndeclaredTool(String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("declaration unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait AccessCallValidator: Send + Sync {
    async fn validate(
        &self,
        workspace_id: &str,
        thing_id: &str,
        tool: &str,
        params: Option<&serde_json::Value>,
    ) -> Result<ValidatedCall, CallError>;
}

pub fn validate_call(
    decl: &DeviceDeclaration,
    tool: &str,
    params: Option<&serde_json::Value>,
    requested_timeout_ms: Option<u64>,
) -> Result<ValidatedCall, CallError> {
    let t = decl
        .tools
        .iter()
        .find(|t| t.name == tool)
        .ok_or_else(|| CallError::UndeclaredTool(tool.to_string()))?
        .clone();
    let params = params.cloned().unwrap_or_else(|| serde_json::json!({}));
    validate_params(&t.parameters, &params).map_err(CallError::InvalidParams)?;
    let max = t.timeout.max_ms.max(1);
    let requested = requested_timeout_ms.unwrap_or(t.timeout.default_ms).max(1);
    let timeout_ms = requested.min(max);
    Ok(ValidatedCall { tool: t, timeout_ms, clamped: timeout_ms != requested })
}
```

Add unit tests for undeclared tool, out-of-range param, and timeout clamp (copy from the earlier draft).

Implement `AccessCallValidator` for `AccessService` in `service.rs`:
```rust
#[async_trait::async_trait]
impl super::validator::AccessCallValidator for AccessService {
    async fn validate(
        &self, workspace_id: &str, thing_id: &str, tool: &str, params: Option<&serde_json::Value>,
    ) -> Result<super::validator::ValidatedCall, super::validator::CallError> {
        let decl = self
            .load_declaration(workspace_id, thing_id)
            .await
            .map_err(|e| super::validator::CallError::Unavailable(e.to_string()))?
            .ok_or_else(|| super::validator::CallError::NoDeclaration(thing_id.into()))?;
        super::validator::validate_call(&decl, tool, params, None)
    }
}
```

- [ ] **Step 2: Tool field and call**

`InvokeActionTool`:
```rust
pub access: Option<Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>>,
```

In `execute`, after the `thing_type` guard:
```rust
if let Some(access) = &self.access {
    access
        .validate(&self.workspace_id, &input.thing_id, &input.action_name, input.params.as_ref())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
}
```

- [ ] **Step 3: Update construction sites**

`ThingToolContext` add `pub access: Option<Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>>`.
Pass `access: runtime.access.clone()` in the two `InvokeActionTool` sites in `tools/thing/mod.rs`.
Add `access: None` (or the available context value) to the 6 sites in `autonomous_factory.rs` / `autonomous_invoke.rs`.
Run `cargo check -p tinyiothub-cloud` and fix every remaining site.

- [ ] **Step 4: Tests**

Add validator unit tests (3). Extend `invoke_action_flow_tests.rs` with a fake validator that returns `UndeclaredTool`; assert the tool result is unsuccessful and contains "not declared".

- [ ] **Step 5: Run and commit**

Run: `cargo test -p runtime access::validator && cargo test -p tinyiothub-cloud invoke_action`
Expected: PASS.

```bash
git add crates/runtime/src/access/validator.rs crates/runtime/src/access/service.rs apps/cloud/src/domains/agent/host/tools/thing/ apps/cloud/src/domains/agent/host/autonomous_factory.rs apps/cloud/src/domains/agent/host/tools/autonomous_invoke.rs apps/cloud/src/tests/invoke_action_flow_tests.rs
git commit -m "feat(e4a): 设备调用走声明校验（未声明/越界拒绝）"
```

---

### Task 12: 验收 runner（core/materialization/migration）、acceptance API 与 driver-check

**Files:**
- Create: `crates/runtime/src/acceptance/{mod.rs,report.rs,runner.rs,test_hub.rs}`
- Create: `crates/runtime/src/acceptance/profiles/{mod.rs,core.rs,materialization.rs,migration.rs}`
- Modify: `crates/runtime/src/lib.rs`
- Create: `crates/driver-check/{Cargo.toml,src/main.rs,tests/e2e.rs}`
- Modify: `apps/cloud/src/domains/driver/declaration/handler.rs` (acceptance route)
- Create: `spec/driver-manifest.schema.json` + gen:check test (in `crates/core/src/driver/manifest.rs` tests)
- Test: inline + integration

**Interfaces:**
- Produces: `Level`, `CheckResult`, `ProfileReport`, `AcceptanceReport`, `run_profiles`; `POST /v1/drivers/{driver}/acceptance`; `driver-check` CLI

- [ ] **Step 1: Report types**

`report.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level { Pass, Fail, Warn, Skip, Manual }

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
        if self.checks.iter().any(|c| c.level == Level::Fail) { Level::Fail }
        else if self.checks.iter().any(|c| c.level == Level::Warn) { Level::Warn }
        else if self.checks.iter().all(|c| c.level == Level::Skip) { Level::Skip }
        else if self.checks.iter().any(|c| c.level == Level::Pass) { Level::Pass }
        else { Level::Manual }
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
    pub fn passed(&self) -> bool {
        self.profiles.iter().all(|p| matches!(p.level(), Level::Pass | Level::Skip | Level::Manual))
    }
}
```

- [ ] **Step 2: Test hub**

`test_hub.rs`: in-memory `Mutex<Vec>` stores implementing `DeclarationProjectionStore`; expose:
```rust
pub struct TestHub { projection: InMemoryProjection }
impl TestHub {
    pub fn new() -> Self;
    pub fn projection(&self) -> &dyn DeclarationProjectionStore;
    pub fn projection_counts(&self) -> (usize, usize);
}
pub fn fixture_thing() -> Thing  // one property + one command
```
Copy the `Thing` fixture from Task 8 tests.

- [ ] **Step 3: Profiles**

`profiles/core.rs`:

```rust
use std::path::Path;
use crate::acceptance::report::{CheckResult, Level, ProfileReport};

pub fn run(dir: &Path) -> ProfileReport {
    let mut checks = Vec::new();
    match crate::access::package::read_package(dir) {
        Ok(pkg) => {
            let schema_ok = tinyiothub_core::driver::manifest::validate_manifest(&pkg.manifest).is_ok();
            checks.push(CheckResult { profile: "core".into(), check: "manifest.schema".into(),
                level: if schema_ok { Level::Pass } else { Level::Fail },
                detail: schema_ok.to_string(), evidence: serde_json::Value::Null });
            checks.push(CheckResult { profile: "core".into(), check: "identity.abi".into(),
                level: if pkg.manifest.identity.abi_version == 2 { Level::Pass } else { Level::Fail },
                detail: format!("abi={}", pkg.manifest.identity.abi_version), evidence: serde_json::Value::Null });
            checks.push(CheckResult { profile: "core".into(), check: "config.schema".into(),
                level: if pkg.manifest.config.is_object() { Level::Pass } else { Level::Fail },
                detail: "config must be an object schema".into(), evidence: serde_json::Value::Null });
            if let Some(bin) = &pkg.binary_path {
                let ok = crate::driver::validator::DriverValidator::validate(bin, "{}").is_ok();
                checks.push(CheckResult { profile: "core".into(), check: "binary.abi_v2".into(),
                    level: if ok { Level::Pass } else { Level::Fail },
                    detail: bin.display().to_string(), evidence: serde_json::Value::Null });
            }
        }
        Err(e) => checks.push(CheckResult { profile: "core".into(), check: "manifest.load".into(),
            level: Level::Fail, detail: e.to_string(), evidence: serde_json::Value::Null }),
    }
    ProfileReport { profile: "core".into(), checks }
}
```

`profiles/migration.rs`:

```rust
use tinyiothub_core::driver::dynamic::{v1_unsupported_message, verify_v2_version};
use crate::acceptance::report::{CheckResult, Level, ProfileReport};

pub fn run() -> ProfileReport {
    let msg = v1_unsupported_message(std::path::Path::new("libv1.so"));
    let checks = vec![
        CheckResult { profile: "migration".into(), check: "v1.rejected".into(),
            level: if msg.contains("tinyiothub_driver_vtable_v2") { Level::Pass } else { Level::Fail },
            detail: msg, evidence: serde_json::Value::Null },
        CheckResult { profile: "migration".into(), check: "v2.version".into(),
            level: if verify_v2_version(2).is_ok() { Level::Pass } else { Level::Fail },
            detail: "ABI v2 accepted".into(), evidence: serde_json::Value::Null },
    ];
    ProfileReport { profile: "migration".into(), checks }
}
```

`profiles/materialization.rs`:

```rust
use std::path::Path;
use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::acceptance::test_hub::{TestHub, fixture_thing};

pub fn run(dir: &Path) -> ProfileReport {
    let mut checks = Vec::new();
    match crate::access::package::read_package(dir) {
        Ok(pkg) => {
            let decl = if pkg.manifest.identity.dynamic_capabilities {
                crate::driver::describe::declaration_from_thing(
                    &fixture_thing(), &pkg.manifest.identity.driver, &pkg.manifest.identity.version)
            } else {
                tinyiothub_core::driver::declaration::fixed_declaration(&pkg.manifest)
            };
            let valid = tinyiothub_core::driver::declaration::validate_device_declaration(&pkg.manifest, &decl);
            checks.push(CheckResult { profile: "materialization".into(), check: "declaration.valid".into(),
                level: if valid.is_ok() { Level::Pass } else { Level::Fail },
                detail: format!("{valid:?}"), evidence: serde_json::Value::Null });
            let hub = TestHub::new();
            let rt = tokio::runtime::Runtime::new().unwrap();
            let res = rt.block_on(crate::access::materialize::materialize_declaration(
                hub.projection(), "test-thing", &decl));
            checks.push(CheckResult { profile: "materialization".into(), check: "projection.write".into(),
                level: if res.is_ok() { Level::Pass } else { Level::Fail },
                detail: format!("{res:?}"), evidence: serde_json::Value::Null });
            let (p, t) = hub.projection_counts();
            checks.push(CheckResult { profile: "materialization".into(), check: "projection.count".into(),
                level: if p == decl.state_fields.len() && t == decl.tools.len() { Level::Pass } else { Level::Fail },
                detail: format!("props={p} tools={t}"), evidence: serde_json::Value::Null });
        }
        Err(e) => checks.push(CheckResult { profile: "materialization".into(), check: "package.read".into(),
            level: Level::Fail, detail: e.to_string(), evidence: serde_json::Value::Null }),
    }
    ProfileReport { profile: "materialization".into(), checks }
}
```

- [ ] **Step 4: Runner + CLI**

`runner.rs`:
```rust
pub fn run_profiles(dir: &std::path::Path, profiles: &[&str]) -> AcceptanceReport {
    let pkg = crate::access::package::read_package(dir);
    let (driver, version) = pkg.as_ref()
        .map(|p| (p.manifest.identity.driver.clone(), p.manifest.identity.version.clone()))
        .unwrap_or_else(|_| ("<unreadable>".into(), "<unreadable>".into()));
    let mut reports = Vec::new();
    for p in profiles {
        match *p {
            "all" => {
                reports.push(profiles::core::run(dir));
                reports.push(profiles::materialization::run(dir));
                reports.push(profiles::migration::run());
            }
            "core" => reports.push(profiles::core::run(dir)),
            "materialization" => reports.push(profiles::materialization::run(dir)),
            "migration" => reports.push(profiles::migration::run()),
            other => panic!("unknown profile {other}"),
        }
    }
    AcceptanceReport { package: dir.display().to_string(), driver, version, profiles: reports }
}
```
Declare `pub mod profiles;` in `acceptance/mod.rs`. Add `pub mod acceptance;` to `runtime/src/lib.rs`.

`crates/driver-check/Cargo.toml`:
```toml
[package]
name = "driver-check"
version = "0.1.0"
edition = "2024"
rust-version.workspace = true

[dependencies]
runtime = { workspace = true }
serde_json = { workspace = true }
```

`src/main.rs`: parse `--package` and repeated `--profile`; default `all`; print JSON and a one-line summary; exit 1 if `!report.passed()`.

- [ ] **Step 5: Acceptance endpoint**

In `declaration/handler.rs` add:
```rust
.route("/{driver}/acceptance", post(run_acceptance))
```
```rust
async fn run_acceptance(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(driver): Path<String>,
) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    let Some(dir) = access.package_dir(&ws, &driver) else {
        return Json(json!({"success": false, "error": "driver not installed"}));
    };
    let report = tinyiothub_runtime::acceptance::runner::run_profiles(&dir, &["all"]);
    let rec = tinyiothub_core::driver::declaration::AcceptanceReportRecord {
        workspace_id: ws.clone(),
        driver_name: report.driver.clone(),
        driver_version: report.version.clone(),
        profile: "all".into(),
        level: if report.passed() { "pass".into() } else { "fail".into() },
        report_json: serde_json::to_string(&report).unwrap_or_default(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = access.acceptance_store.insert(&rec).await {
        tracing::warn!(error = %e, "acceptance report persist failed");
    }
    Json(json!({"success": true, "report": report}))
}
```
`AccessService.acceptance_store` is defined in Task 10; no extra AppState field is needed.

- [ ] **Step 6: Schema gen:check**

Add to `crates/core/src/driver/manifest.rs` tests:
```rust
#[test]
fn schema_file_matches_types() {
    let schema = schemars::schema_for!(DriverManifest);
    let generated = serde_json::to_string_pretty(&schema).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/driver-manifest.schema.json");
    let checked_in = std::fs::read_to_string(&path).unwrap();
    assert_eq!(checked_in.trim(), generated.trim(), "run `cargo test -p core generate_manifest_schema -- --ignored` and commit the file");
}
```
Add an ignored generator test:
```rust
#[test]
#[ignore]
fn generate_manifest_schema() {
    let schema = schemars::schema_for!(DriverManifest);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/driver-manifest.schema.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();
}
```
Run: `cargo test -p core generate_manifest_schema -- --ignored`, then commit `spec/driver-manifest.schema.json`.

- [ ] **Step 7: E2E test**

`crates/driver-check/tests/e2e.rs`:
```rust
#[test]
fn demo_declarative_passes_core_materialization_migration() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../drivers/demo-declarative");
    let report = tinyiothub_runtime::acceptance::runner::run_profiles(&dir, &["all"]);
    assert!(report.passed(), "{}", serde_json::to_string_pretty(&report).unwrap());
    assert_eq!(report.driver, "demo-declarative");
}
```

- [ ] **Step 8: Run and commit**

Run:
```bash
cargo test -p runtime acceptance
cargo test -p driver-check
cargo run -p driver-check -- --package drivers/demo-declarative --profile all
```
Expected: all pass; CLI exit 0.

```bash
git add crates/runtime/src/acceptance/ crates/runtime/src/lib.rs crates/driver-check/ spec/ crates/core/src/driver/manifest.rs apps/cloud/src/domains/driver/declaration/handler.rs apps/cloud/src/state.rs apps/cloud/src/shared/service_manager.rs
git commit -m "feat(e4a): 验收 runner（core/materialization/migration）+ acceptance API + driver-check"
```

---

### Task 13: E4a 收口验证

- [ ] **Step 1: Format and lint**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: clean.

- [ ] **Step 2: Test set**

> `cargo build -p demo-declarative` must run before `cargo test -p driver-check`: the
> driver-check binary test asserts on the built cdylib and fails (rather than skips)
> when it is missing.

```bash
cargo build -p demo-declarative
cargo test -p core
cargo test -p runtime
cargo test -p db
cargo test -p demo-declarative
cargo test -p driver-check
cargo test -p tinyiothub-cloud access_declaration invoke_action
```
Expected: all pass.

- [ ] **Step 3: Exit verification**

```bash
cargo build -p demo-declarative
cargo run -p driver-check -- --package drivers/demo-declarative --profile all
cargo test -p core driver::dynamic::tests::v1_message_names_the_artifact
```
Expected: CLI exit 0; v1 message test passes.

- [ ] **Step 4: Execution record**

Create `docs/superpowers/plans/2026-10-21-e4a-execution-record.md` with commit range, command outputs (pass counts), deviations/open items handed to E4b-e (scene/MCP creation paths, sampling, resources, maps).

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-10-21-e4a-execution-record.md
git commit -m "docs(e4a): 执行记录与验证结果"
```

---

## Self-Review

**Spec coverage (E4a):**
- manifest v1 + version negotiation → Task 1
- ABI v2 + v1 rejection → Tasks 2 + 7
- 双层声明（describe → device_declarations）→ Tasks 3 + 8 + 10
- 3 built-ins v2 + manifests → Tasks 7 + 8
- demo-declarative → Task 9
- 安装/物化 API（属性/命令；坐标 E4d）→ Tasks 4 + 6 + 10
- core / materialization / migration profiles + test hub + CLI → Task 12
- 调用校验（业务拒绝/超时钳制/未声明拒绝）→ Task 11
- E4b-e 非目标 → 未在本计划

**Placeholder scan:** no TBD/TODO. All scaffold files (`device.rs`/`materialize.rs`/`service.rs`/`validator.rs` created empty in Task 5) are filled by Tasks 6/10/11 before the plan finishes; each creating task states its contents.

**Type consistency:** `DriverManifest`, `DeviceDeclaration`, `StateFieldDecl`, `ManifestTool`, `CancelToken`, `DriverVTableV2`, `LoadedPackage`, `InstalledPackage`, `AccessService`, `ValidatedCall`, `Level` are defined once. Port traits live in `runtime::ports`; `db` does not depend on runtime — adapters live in `apps/cloud/src/shared/access_ports.rs`.

**Known sequencing:** Task 7 is a workspace-wide signature change; after Step 2 run `cargo check` and update every listed impl before Task 8. `long_task` cooperates with the ABI cancel handle; wiring `stop_device` to cancel in-flight calls remains E4c.
