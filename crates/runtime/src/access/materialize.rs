//! E4a 声明物化：把设备声明写入属性/命令投影，并标记未声明的旧条目为 stale。

use crate::ports::DeclarationProjectionStore;
use tinyiothub_core::driver::declaration::DeviceDeclaration;

/// 按「属性 → 命令 → stale」的顺序物化声明。
///
/// `mark_stale` 的 keep 集合为声明中的名称，删除由 store 负责。
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
            self.props
                .lock()
                .unwrap()
                .push((thing_id.into(), fields.iter().map(|f| f.name.clone()).collect()));
            Ok(())
        }
        async fn upsert_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
            self.cmds
                .lock()
                .unwrap()
                .push((thing_id.into(), tools.iter().map(|t| t.name.clone()).collect()));
            Ok(())
        }
        async fn mark_stale(&self, thing_id: &str, keep_p: &[String], keep_c: &[String]) -> Result<(), String> {
            self.stale
                .lock()
                .unwrap()
                .push((thing_id.into(), keep_p.to_vec(), keep_c.to_vec()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn writes_properties_commands_and_stale_set() {
        let store = FakeStore::default();
        let decl = DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![StateFieldDecl {
                name: "level".into(),
                data_type: "integer".into(),
                unit: None,
                range: None,
                read_only: false,
                required: true,
                source: None,
            }],
            tools: vec![ManifestTool {
                name: "set_level".into(),
                kind: ToolKindDecl::Action,
                target_field: None,
                parameters: serde_json::json!({"type":"object"}),
                timeout: ToolTimeout::default(),
                safety: ToolSafetyDecl::Control,
                resources: vec![],
                cancellable: true,
            }],
        };
        materialize_declaration(&store, "t1", &decl).await.unwrap();
        assert_eq!(store.props.lock().unwrap()[0].1, vec!["level"]);
        assert_eq!(store.cmds.lock().unwrap()[0].1, vec!["set_level"]);
        assert_eq!(store.stale.lock().unwrap()[0].1, vec!["level"]);
    }
}
