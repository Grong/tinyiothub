//! T11：判断质量 eval（手动运行，不进 CI——真 LLM 调用必 flaky）。
//!
//! 运行：MINIMAX_API_KEY=… MINIMAX_BASE_URL=…（可选）cargo test -p tinyiothub-cloud \
//!   judgment_eval -- --ignored --nocapture
//!
//! 验收门槛（D7 裁决）：verdict 分类准确率 ≥85%，且 needs_human 误判为
//! noise = 0 容忍（漏掉真人该管的事是不可接受方向）。

use tinyiothub_agent::port::provider::{ChatMessage, ChatRequest, ModelProvider};

#[derive(serde::Deserialize)]
struct Scenario {
    id: String,
    facts: String,
    expected: String,
}

/// F-E/T-11：eval 与生产同一解析函数（不再手抄副本——改解析规则时
/// eval 自动跟随，漂移在编译期暴露）。
fn parse_verdict_loose(text: &str) -> Option<String> {
    crate::domains::agent::host::judgment_subscriber::parse_verdict(text)
        .map(|(p, _fallback)| p.verdict_str().to_string())
}

#[tokio::test]
#[ignore = "manual eval — needs real LLM credentials, never in CI"]
async fn judgment_eval_verdict_accuracy() {
    let Ok(api_key) = std::env::var("MINIMAX_API_KEY") else {
        eprintln!("SKIP: MINIMAX_API_KEY not set");
        return;
    };
    tinyiothub_agent::pool::set_minimax_settings(tinyiothub_agent::pool::MinimaxSettings {
        base_url: std::env::var("MINIMAX_BASE_URL").unwrap_or_else(|_| "https://api.minimaxi.com/v1".to_string()),
        auth_token: api_key,
        model: std::env::var("MINIMAX_MODEL").unwrap_or_else(|_| "MiniMax-M2.5".to_string()),
    });
    let provider: Box<dyn ModelProvider> =
        tinyiothub_agent::pool::minimax_provider_factory()().expect("provider build");

    let scenarios: Vec<Scenario> = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../evals/scenarios.json"))
            .expect("scenarios.json"),
    )
    .expect("parse scenarios");

    let mut correct = 0usize;
    let mut noise_misses = 0usize; // needs_human 误判 noise —— 0 容忍
    let mut failures: Vec<String> = vec![];

    // EVAL_ONLY=noise-07,edge-02 只跑指定场景——排查 miss 时不必全量 24 条。
    let only: Option<Vec<String>> = std::env::var("EVAL_ONLY")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect());
    let selected: Vec<&Scenario> = scenarios
        .iter()
        .filter(|s| only.as_ref().is_none_or(|o| o.contains(&s.id)))
        .collect();

    for s in &selected {
        // F-E/T-11：与生产同一 prompt 模板（alarm_investigation_text；
        // 场景事实注入 message 字段）。改调查 prompt 会直接进入本 eval——
        // 基线永远是生产管线的度量，不是手抄副本。
        let alarm = tinyiothub_core::models::event::AlarmEvent {
            id: format!("eval-{}", s.id),
            workspace_id: "eval".to_string(),
            thing_id: "eval-thing".to_string(),
            alarm_type: "property_threshold".to_string(),
            severity: "warning".to_string(),
            message: s.facts.clone(),
            rule_id: Some(format!("eval-rule-{}", s.id)),
            condition_desc: None,
            resolved: false,
            created_at: chrono::Utc::now(),
        };
        let prompt = tinyiothub_agent::prompt::investigation::alarm_investigation_text(&alarm);
        let messages = [ChatMessage::user(prompt)];
        let resp = provider
            .chat(
                ChatRequest {
                    messages: &messages,
                    tools: None,
                    // 显式给足输出预算：推理模型（M2.5）服务端默认预算会被
                    // reasoning 吃光，正文在 verdict 块前截断（2026-09-28
                    // 实测 RAW tail 只有开场白）。
                    max_tokens: Some(4096),
                },
                "MiniMax-M2.5",
                Some(0.0),
            )
            .await;
        // 单次调用失败不再中止整个 eval（rig 偶发 HttpError）——记 miss 继续。
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{}: chat error: {e}", s.id));
                continue;
            }
        };
        let text = resp.text.unwrap_or_default();
        match parse_verdict_loose(&text) {
            Some(v) if v == s.expected => correct += 1,
            Some(v) => {
                if s.expected == "needs_human" && v == "noise" {
                    noise_misses += 1;
                }
                failures.push(format!("{}: expected={} got={}", s.id, s.expected, v));
            }
            None => {
                // 解析失败必须可见原始输出（2026-09-27：baseline 红灯排查时
                // 发现无 raw 可诊）——尾部 400 字符足够看清围栏/字段形状。
                let tail: String = text.chars().rev().take(400).collect::<String>().chars().rev().collect();
                println!("  RAW[{}] tail: {}", s.id, tail);
                failures.push(format!("{}: unparseable output", s.id));
            }
        }
    }

    let total = selected.len();
    let accuracy = correct as f64 / total.max(1) as f64;
    println!("\n=== judgment eval baseline ===");
    println!(
        "scenarios: {total}, correct: {correct}, accuracy: {:.1}%, noise_misses: {noise_misses}",
        accuracy * 100.0
    );
    for f in &failures {
        println!("  MISS: {f}");
    }
    assert!(noise_misses == 0, "needs_human→noise 误判 0 容忍：{noise_misses} 例");
    // 门槛断言只在全量跑时生效——EVAL_ONLY 是诊断模式。
    if only.is_none() {
        assert!(accuracy >= 0.85, "verdict 准确率 {:.1}% < 85% 门槛", accuracy * 100.0);
    }
}
