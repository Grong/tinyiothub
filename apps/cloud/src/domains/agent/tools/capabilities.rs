//! Composition-root capability resolution and the run-level capability gate.
//!
//! `ToolDescriptor.required_capabilities` is the declaration; this module
//! resolves what the running provider/model actually offers and writes the
//! `capability.unavailable` event before any model call when a required
//! capability is missing. A capability failure creates zero Effects.

use chrono::Utc;
use serde_json::json;
use tinyiothub_skills::registry::Capability;
use tinyiothub_storage::runtime_event::RuntimeEventInput;

use crate::domains::agent::runtime::stream::RuntimeRecorder;

/// Resolve the available capability set from the provider/model port plus
/// explicit configuration (for example a configured video source). Video is
/// not inferred from `supports_vision` alone: a video source must be
/// configured, and the descriptor gate additionally requires vision.
pub fn resolve_capabilities(
    supports_vision: bool,
    configured: impl IntoIterator<Item = Capability>,
) -> Vec<Capability> {
    let mut available: Vec<Capability> = configured.into_iter().collect();
    if supports_vision && !available.contains(&Capability::Vision) {
        available.push(Capability::Vision);
    }
    available
}

/// Persist the pre-model `capability.unavailable` event. Best-effort: the
/// caller still returns the capability error even if the recorder is absent or
/// the write fails, because failing closed matters more than the event.
pub async fn emit_capability_unavailable(
    recorder: Option<&RuntimeRecorder>,
    workspace_id: &str,
    run_id: &str,
    task_id: Option<&str>,
    missing: &[Capability],
    tool_name: &str,
) -> Result<(), String> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let missing_names: Vec<&str> = missing.iter().map(|capability| capability.as_str()).collect();
    recorder
        .record(RuntimeEventInput {
            event_id: format!("capability-unavailable:{run_id}:{tool_name}"),
            event_type: "capability.unavailable".to_string(),
            occurred_at: Utc::now(),
            workspace_id: workspace_id.to_string(),
            task_id: task_id.map(str::to_string),
            run_id: Some(run_id.to_string()),
            tool_call_id: None,
            job_id: None,
            approval_id: None,
            device_id: None,
            source_id: None,
            source_seq: None,
            evidence_id: None,
            data: json!({
                "missingCapabilities": missing_names,
                "tool": tool_name,
                "recovery": "配置具备相应视觉/视频能力的 provider 或来源后创建新的 Run；历史失败记录不会改写",
            }),
        })
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_is_only_granted_by_the_provider_port_or_explicit_configuration() {
        assert_eq!(resolve_capabilities(false, []), Vec::<Capability>::new());
        assert_eq!(resolve_capabilities(true, []), vec![Capability::Vision]);
        assert_eq!(
            resolve_capabilities(true, [Capability::Video]),
            vec![Capability::Video, Capability::Vision]
        );
        // Video without vision is not usable.
        let available = resolve_capabilities(false, [Capability::Video]);
        assert!(!Capability::Video.is_satisfied_by(&available));
    }
}
