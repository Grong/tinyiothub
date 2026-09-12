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

fn parse_verdict_loose(text: &str) -> Option<String> {
    // 与 judgment_subscriber::parse_verdict 同规则（围栏块 → 宽松 JSON）
    if let Some(start) = text.rfind("```json") {
        let block = &text[start + 7..];
        if let Some(end) = block.find("```")
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(block[..end].trim())
        {
            return v.get("verdict")?.as_str().map(str::to_string);
        }
    }
    if let Some(start) = text.rfind('{') {
        let candidate = &text[start..];
        if candidate.contains("\"verdict\"")
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(candidate.trim())
        {
            return v.get("verdict")?.as_str().map(str::to_string);
        }
    }
    None
}

#[tokio::test]
#[ignore = "manual eval — needs real LLM credentials, never in CI"]
async fn judgment_eval_verdict_accuracy() {
    let Ok(api_key) = std::env::var("MINIMAX_API_KEY") else {
        eprintln!("SKIP: MINIMAX_API_KEY not set");
        return;
    };
    tinyiothub_agent::pool::set_minimax_settings(tinyiothub_agent::pool::MinimaxSettings {
        base_url: std::env::var("MINIMAX_BASE_URL")
            .unwrap_or_else(|_| "https://api.minimaxi.com/v1".to_string()),
        auth_token: api_key,
        model: std::env::var("MINIMAX_MODEL").unwrap_or_else(|_| "MiniMax-M2.5".to_string()),
    });
    let provider: Box<dyn ModelProvider> = tinyiothub_agent::pool::minimax_provider_factory()()
        .expect("provider build");

    let scenarios: Vec<Scenario> = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../evals/scenarios.json"))
            .expect("scenarios.json"),
    )
    .expect("parse scenarios");

    let mut correct = 0usize;
    let mut noise_misses = 0usize; // needs_human 误判 noise —— 0 容忍
    let mut failures: Vec<String> = vec![];

    for s in &scenarios {
        // 与生产同 prompt 模板（alarm_investigation_text 同结构；事实直接注入）
        let prompt = format!(
            "调查报警并给出处置判断。报警事实：{}。\
             请判断：noise（正常波动/无需处理）/ self_healable（可自愈，给出建议动作）/ \
             needs_human（需要人工介入）。结束前输出一行结构化结论：\
             ```json {{\"verdict\": \"...\", \"reason\": \"一句人话理由\"}}```",
            s.facts
        );
        let messages = [ChatMessage::user(prompt)];
        let resp = provider
            .chat(ChatRequest { messages: &messages, tools: None }, "MiniMax-M2.5", Some(0.0))
            .await
            .expect("chat");
        let text = resp.text.unwrap_or_default();
        match parse_verdict_loose(&text) {
            Some(v) if v == s.expected => correct += 1,
            Some(v) => {
                if s.expected == "needs_human" && v == "noise" {
                    noise_misses += 1;
                }
                failures.push(format!("{}: expected={} got={}", s.id, s.expected, v));
            }
            None => failures.push(format!("{}: unparseable output", s.id)),
        }
    }

    let total = scenarios.len();
    let accuracy = correct as f64 / total as f64;
    println!("\n=== judgment eval baseline ===");
    println!("scenarios: {total}, correct: {correct}, accuracy: {:.1}%, noise_misses: {noise_misses}", accuracy * 100.0);
    for f in &failures {
        println!("  MISS: {f}");
    }
    assert!(noise_misses == 0, "needs_human→noise 误判 0 容忍：{noise_misses} 例");
    assert!(accuracy >= 0.85, "verdict 准确率 {:.1}% < 85% 门槛", accuracy * 100.0);
}
