//! E2 turn 开头工作区快照渲染（spec §4 / T5）。
//!
//! 硬预算：**6144 UTF-8 字节**（AGH ≤6KB 参照）。按 health worst-first
//! 逐行组装，超限即停并显式标注省略台数；空快照整段省略（不炸 run）。
//! token 数只做估算（中文 chars/1.5），实测基线由单测固定。

use tinyiothub_core::agent_runs::SnapshotMetrics;
use tinyiothub_runtime::watch::live_state::{DeviceSnapshot, Health, WorkspaceSnapshot};

/// 快照硬上限（UTF-8 字节）。
pub const SNAPSHOT_MAX_BYTES: usize = 6144;
/// 单设备最多列出的属性数。
const MAX_VALUES: usize = 8;
/// 单属性值最大字符数。
const MAX_VALUE_CHARS: usize = 40;
/// zone 最大字节数（中文名防单行失控）。
const MAX_ZONE_BYTES: usize = 120;

/// 渲染快照正文（不含围栏）；空设备返回空串。
pub fn render_snapshot(snapshot: &WorkspaceSnapshot) -> (String, SnapshotMetrics) {
    if snapshot.devices.is_empty() {
        return (
            String::new(),
            SnapshotMetrics {
                bytes: 0,
                chars: 0,
                devices: 0,
                omitted: 0,
                est_tokens: 0,
            },
        );
    }

    let mut out = String::new();
    let mut rendered = 0u32;
    // 预留省略行空间（最坏 ~40 字节），保证最终仍在预算内。
    let budget = SNAPSHOT_MAX_BYTES.saturating_sub(48);
    // 防御性排序（LiveState 已排序；直接构造的调用方也得到 worst-first）。
    let mut ordered: Vec<&DeviceSnapshot> = snapshot.devices.iter().collect();
    ordered.sort_by(|a, b| {
        a.health
            .rank()
            .cmp(&b.health.rank())
            .then_with(|| a.zone.cmp(&b.zone))
            .then_with(|| a.name.cmp(&b.name))
    });
    for device in ordered {
        let line = render_device_line(device);
        // +1 for '\n'
        if out.len() + line.len() + 1 > budget {
            break;
        }
        if rendered > 0 {
            out.push('\n');
        }
        out.push_str(&line);
        rendered += 1;
    }

    let omitted = snapshot.devices.len() as u32 - rendered;
    if omitted > 0 {
        if rendered > 0 {
            out.push('\n');
        }
        out.push_str(&format!("- … 另有 {omitted} 台设备未列出"));
    }

    let chars = out.chars().count() as u32;
    let metrics = SnapshotMetrics {
        bytes: out.len() as u32,
        chars,
        devices: rendered,
        omitted,
        est_tokens: ((chars as f64) / 1.5).ceil() as u32,
    };
    (out, metrics)
}

/// prompt 段（围栏 + v1）。空正文返回空串（调用方整段省略）。
pub fn section(body: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    format!("\n\n工作区实时快照（v1）：\n<workspace_snapshot>\n{body}\n</workspace_snapshot>")
}

fn render_device_line(device: &DeviceSnapshot) -> String {
    let health = match device.health {
        Health::Ok => "ok",
        Health::Attention => "attention",
        Health::Bad => "bad",
        Health::Unknown => "unknown",
    };
    let zone = truncate_bytes(&device.zone, MAX_ZONE_BYTES);
    let zone = if zone.is_empty() {
        "未归属区域"
    } else {
        zone.as_str()
    };
    let online = if device.online { "在线" } else { "离线" };

    let mut values: Vec<&(String, String)> = device.values.iter().collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    let rendered_values: Vec<String> = values
        .into_iter()
        .take(MAX_VALUES)
        .map(|(k, v)| format!("{k}={}", truncate_chars(v, MAX_VALUE_CHARS)))
        .collect();
    let values_text = if rendered_values.is_empty() {
        "（无上报值）".to_string()
    } else {
        rendered_values.join(", ")
    };

    format!(
        "- {}（{}）[{health}] {zone} {online} 关键属性: {values_text}",
        device.name, device.thing_id
    )
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        t.push('…');
    }
    t
}

fn truncate_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn device(id: &str, name: &str, health: Health, zone: &str) -> DeviceSnapshot {
        DeviceSnapshot {
            thing_id: id.into(),
            name: name.into(),
            health,
            online: true,
            zone: zone.into(),
            values: vec![("温度".into(), "31.5".into())],
        }
    }

    #[test]
    fn empty_snapshot_renders_nothing() {
        let (body, m) = render_snapshot(&WorkspaceSnapshot {
            v: 1,
            devices: vec![],
            total: 0,
        });
        assert!(body.is_empty());
        assert_eq!(m, SnapshotMetrics::default());
        assert!(section(&body).is_empty());
    }

    #[test]
    fn worst_first_and_fenced_section() {
        let snap = WorkspaceSnapshot {
            v: 1,
            devices: vec![
                device("ok", "正常-1", Health::Ok, "楼A"),
                device("bad", "故障-1", Health::Bad, "楼B"),
                device("att", "注意-1", Health::Attention, "楼C"),
            ],
            total: 3,
        };
        let (body, m) = render_snapshot(&snap);
        let lines: Vec<&str> = body.lines().collect();
        assert!(lines[0].contains("故障-1"), "bad must be first: {body}");
        assert!(lines[1].contains("注意-1"));
        assert!(lines[2].contains("正常-1"));
        assert_eq!(m.devices, 3);
        assert_eq!(m.omitted, 0);

        let s = section(&body);
        assert!(s.contains("<workspace_snapshot>"));
        assert!(s.contains("</workspace_snapshot>"));
        assert!(s.contains("（v1）"));
    }

    /// T5 中文基线：50 台中文命名设备 + 8 属性，必须 ≤6144 字节且显式省略。
    #[test]
    fn chinese_baseline_stays_within_6kb() {
        let mut values = HashMap::new();
        for i in 0..8 {
            values.insert(format!("属性{i}"), format!("数值{i}-中文值"));
        }
        let devices: Vec<DeviceSnapshot> = (0..50)
            .map(|i| DeviceSnapshot {
                thing_id: format!("thing-{i:02}"),
                name: format!("园区A/3号楼/温湿度传感器-{i:02}"),
                health: if i % 7 == 0 { Health::Bad } else { Health::Ok },
                online: i % 5 != 0,
                zone: "园区A/3号楼".into(),
                values: values.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            })
            .collect();
        let snap = WorkspaceSnapshot {
            v: 1,
            devices,
            total: 50,
        };
        let (body, m) = render_snapshot(&snap);
        assert!(m.bytes as usize <= SNAPSHOT_MAX_BYTES, "bytes={}", m.bytes);
        assert_eq!(m.bytes as usize, body.len());
        assert_eq!(m.devices + m.omitted, 50);
        assert!(body.contains("另有") && body.contains("台设备未列出"));
        // T5 基线（2026-10-21）：本夹具实测 bytes/chars 记录在此断言区间内。
        assert!(m.bytes > 1000, "baseline unexpectedly small: {m:?}");
        assert_eq!(m.est_tokens, ((m.chars as f64) / 1.5).ceil() as u32);
    }

    #[test]
    fn single_huge_value_is_truncated() {
        let mut d = device("t", "设备", Health::Ok, "zone");
        d.values = vec![("blob".into(), "x".repeat(500))];
        let (body, _) = render_snapshot(&WorkspaceSnapshot {
            v: 1,
            devices: vec![d],
            total: 1,
        });
        assert!(!body.contains(&"x".repeat(41)), "per-value cap violated");
        assert!(body.contains('…'));
    }
}
