//! E4c resource resolution (spec 2026-10-22-e4c §2.2).
//!
//! The access layer owns the declaration and package manifest; the agent
//! arbitration registry owns the claim table. This module is the narrow
//! bridge: it turns a declared tool's `resources` ids into [`ResourceClaim`]s
//! (mode/capacity resolved from the package manifest's `resources[]`), and
//! the cloud composition root converts those into agent `ResourceSpec`s.
//!
//! Unknown resource ids fail closed: a declaration that names a resource the
//! package never declared is rejected rather than silently unprotected.
//! Tools with no declared resources claim the synthetic [`DEVICE_RESOURCE`]
//! exclusive resource, preserving E1's per-device mutual exclusion.

use async_trait::async_trait;
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::manifest::{
    ManifestTool, ResourceDecl, ResourceMode as DeclaredResourceMode, ToolKindDecl,
};

/// 未声明 resources 的变更类工具所 claim 的合成资源 id。与仲裁注册表
/// 同源（E4c 之后注册表也住在 runtime，不再有两份常量）。
pub use crate::arbitration::DEVICE_RESOURCE;

/// 一次调用的资源需求（声明解析结果）。`exclusive` 与 `capacity` 的语义
/// 与 agent 的 `ResourceSpec` 一致，但 runtime 不引入 agent 的仲裁类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceClaim {
    pub id: String,
    pub exclusive: bool,
    /// shared 时有效；exclusive 恒按 1 处理。
    pub capacity: u32,
}

/// 工具层解析资源声明的窄接缝。`None` 接线的工具保持 E1 合成设备资源。
#[async_trait]
pub trait ResourceResolver: Send + Sync {
    /// 解析一次 action 调用（工具名 = 声明中的工具名）的资源。
    async fn resolve_action(
        &self,
        workspace_id: &str,
        thing_id: &str,
        tool_name: &str,
    ) -> Result<Vec<ResourceClaim>, String>;

    /// 解析一次 set 调用（属性名集合）的资源：并入所有 target_field 命中
    /// 的 `kind=set` 工具声明。
    async fn resolve_properties(
        &self,
        workspace_id: &str,
        thing_id: &str,
        properties: &[String],
    ) -> Result<Vec<ResourceClaim>, String>;
}

/// 合成设备资源（E1 每设备互斥）。
pub fn device_claim() -> ResourceClaim {
    ResourceClaim {
        id: DEVICE_RESOURCE.to_string(),
        exclusive: true,
        capacity: 1,
    }
}

/// 解析单个声明工具的资源：未知 id 拒绝；空声明回落合成设备资源。
pub fn resolve_tool_claims(resources: &[ResourceDecl], tool: &ManifestTool) -> Result<Vec<ResourceClaim>, String> {
    let mut claims = Vec::with_capacity(tool.resources.len());
    for id in &tool.resources {
        claims.push(claim_for(resources, id)?);
    }
    if claims.is_empty() {
        claims.push(device_claim());
    }
    Ok(normalize(claims))
}

/// 解析一组属性写入的资源：union 所有 `kind=set` 且 target_field 命中的
/// 工具声明。未声明任何资源的工具按合成设备资源计入（保留 E1 语义）；
/// 没有对应 set 工具的属性同样按设备资源处理（fail-closed）。
pub fn resolve_property_claims(
    resources: &[ResourceDecl],
    decl: &DeviceDeclaration,
    properties: &[String],
) -> Result<Vec<ResourceClaim>, String> {
    let mut claims = Vec::new();
    let mut undeclared_property = false;
    for property in properties {
        let mut matched = false;
        for tool in decl
            .tools
            .iter()
            .filter(|t| t.kind == ToolKindDecl::Set && t.target_field.as_deref() == Some(property.as_str()))
        {
            matched = true;
            if tool.resources.is_empty() {
                claims.push(device_claim());
            } else {
                for id in &tool.resources {
                    claims.push(claim_for(resources, id)?);
                }
            }
        }
        undeclared_property |= !matched;
    }
    if claims.is_empty() || undeclared_property {
        claims.push(device_claim());
    }
    Ok(normalize(claims))
}

fn claim_for(resources: &[ResourceDecl], id: &str) -> Result<ResourceClaim, String> {
    let declared = resources
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("unknown resource '{id}'"))?;
    Ok(ResourceClaim {
        id: id.to_string(),
        exclusive: declared.mode == DeclaredResourceMode::Exclusive,
        capacity: declared.capacity,
    })
}

/// 稳定排序 + 去重（同一 id 在声明中重复出现不得 self-busy）。
fn normalize(mut claims: Vec<ResourceClaim>) -> Vec<ResourceClaim> {
    claims.sort_by(|a, b| a.id.cmp(&b.id));
    claims.dedup_by(|a, b| a.id == b.id);
    claims
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::driver::manifest::{ResourceMode, StateFieldDecl, ToolSafetyDecl, ToolTimeout};

    fn resources() -> Vec<ResourceDecl> {
        vec![
            ResourceDecl {
                id: "actuator".into(),
                mode: ResourceMode::Exclusive,
                capacity: 1,
            },
            ResourceDecl {
                id: "camera".into(),
                mode: ResourceMode::Shared,
                capacity: 3,
            },
        ]
    }

    fn tool(name: &str, kind: ToolKindDecl, target: Option<&str>, resources: &[&str]) -> ManifestTool {
        ManifestTool {
            name: name.into(),
            kind,
            target_field: target.map(str::to_string),
            parameters: serde_json::json!({"type": "object"}),
            timeout: ToolTimeout::default(),
            safety: ToolSafetyDecl::Control,
            resources: resources.iter().map(|s| s.to_string()).collect(),
            cancellable: true,
        }
    }

    fn declaration(tools: Vec<ManifestTool>) -> DeviceDeclaration {
        DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: ["level", "mode", "temp"]
                .iter()
                .map(|name| StateFieldDecl {
                    name: name.to_string(),
                    data_type: "integer".into(),
                    unit: None,
                    range: None,
                    read_only: false,
                    required: false,
                    source: None,
                })
                .collect(),
            tools,
        }
    }

    #[test]
    fn action_resolves_mode_and_capacity_sorted_and_deduped() {
        // 乱序 + 重复：排序去重后 actuator(exclusive) 在前、camera(shared 3) 在后。
        let tool = tool("snap", ToolKindDecl::Action, None, &["camera", "actuator", "camera"]);
        let claims = resolve_tool_claims(&resources(), &tool).unwrap();
        assert_eq!(
            claims,
            vec![
                ResourceClaim {
                    id: "actuator".into(),
                    exclusive: true,
                    capacity: 1,
                },
                ResourceClaim {
                    id: "camera".into(),
                    exclusive: false,
                    capacity: 3,
                },
            ]
        );
    }

    #[test]
    fn action_with_empty_resources_claims_device_resource() {
        let claims = resolve_tool_claims(&resources(), &tool("echo", ToolKindDecl::Action, None, &[])).unwrap();
        assert_eq!(claims, vec![device_claim()]);
        assert_eq!(claims[0].id, DEVICE_RESOURCE);
    }

    #[test]
    fn action_unknown_resource_fails_closed() {
        let err = resolve_tool_claims(
            &resources(),
            &tool("ghost", ToolKindDecl::Action, None, &["not-declared"]),
        )
        .unwrap_err();
        assert_eq!(err, "unknown resource 'not-declared'");
    }

    #[test]
    fn set_unions_matching_set_tools_only() {
        let decl = declaration(vec![
            tool("set_level", ToolKindDecl::Set, Some("level"), &["actuator"]),
            tool("set_mode", ToolKindDecl::Set, Some("mode"), &["camera"]),
            // kind=action 即使 target 命中也不参与 set 解析。
            tool("calibrate", ToolKindDecl::Action, Some("level"), &["camera"]),
            tool("set_temp", ToolKindDecl::Set, Some("temp"), &["camera"]),
        ]);
        let claims = resolve_property_claims(&resources(), &decl, &["mode".into(), "level".into()]).unwrap();
        assert_eq!(
            claims.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["actuator", "camera"]
        );
    }

    #[test]
    fn set_empty_resources_claims_device_resource() {
        let decl = declaration(vec![tool("set_level", ToolKindDecl::Set, Some("level"), &[])]);
        let claims = resolve_property_claims(&resources(), &decl, &["level".into()]).unwrap();
        assert_eq!(claims, vec![device_claim()]);
    }

    #[test]
    fn set_without_matching_tool_claims_device_resource() {
        let decl = declaration(vec![tool("set_level", ToolKindDecl::Set, Some("level"), &["actuator"])]);
        let claims = resolve_property_claims(&resources(), &decl, &["unknown_prop".into()]).unwrap();
        assert_eq!(claims, vec![device_claim()]);
    }

    #[test]
    fn set_unknown_resource_fails_closed() {
        let decl = declaration(vec![tool("set_level", ToolKindDecl::Set, Some("level"), &["ghost"])]);
        let err = resolve_property_claims(&resources(), &decl, &["level".into()]).unwrap_err();
        assert_eq!(err, "unknown resource 'ghost'");
    }

    #[test]
    fn set_mixed_declared_and_unmanaged_properties_stay_device_exclusive() {
        // level 声明 actuator，temp 没有对应 set 工具 → 整次调用仍须设备互斥。
        let decl = declaration(vec![tool("set_level", ToolKindDecl::Set, Some("level"), &["actuator"])]);
        let claims = resolve_property_claims(&resources(), &decl, &["level".into(), "temp".into()]).unwrap();
        let ids: Vec<_> = claims.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec![DEVICE_RESOURCE, "actuator"]);
    }
}
