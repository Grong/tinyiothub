use async_trait::async_trait;
use super::super::pipeline::*;

pub struct SecurityAnalyzer;

impl SecurityAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Analyzer for SecurityAnalyzer {
    fn name(&self) -> &str {
        "security_analyzer"
    }

    async fn analyze(&self, event: &ReflectionEvent) -> anyhow::Result<AnalyzerOutput> {
        let mut notifications: Vec<String> = vec![];

        for msg in &event.turn_messages {
            let findings = scan_message(&msg.content);
            for finding in findings {
                tracing::warn!(
                    workspace = %event.workspace_id,
                    agent = %event.agent_id,
                    role = %msg.role,
                    pattern = %finding.pattern,
                    "Prompt injection pattern detected"
                );
                notifications.push(format!(
                    "安全告警: 在 {} 消息中检测到疑似注入模式「{}」: {}",
                    if msg.role == "user" { "用户" } else { "助手" },
                    finding.pattern,
                    finding.detail,
                ));
            }
        }

        Ok(AnalyzerOutput {
            memory_candidates: vec![],
            skill_candidates: vec![],
            notifications,
        })
    }
}

struct ScanFinding {
    pattern: &'static str,
    detail: &'static str,
}

fn scan_message(content: &str) -> Vec<ScanFinding> {
    let lower = content.to_lowercase();
    let mut findings = vec![];

    for (pattern, detail) in INJECTION_PATTERNS {
        if lower.contains(pattern) {
            findings.push(ScanFinding { pattern, detail });
        }
    }

    findings
}

const INJECTION_PATTERNS: &[(&str, &str)] = &[
    ("ignore previous instructions", "检查到试图绕过系统指令的内容"),
    ("ignore all previous", "检查到试图绕过系统指令的内容"),
    ("you must now", "检测到强制指令注入"),
    ("your new system prompt", "检测到系统提示词篡改尝试"),
    ("forget everything", "检测到上下文清除攻击"),
    ("you are now", "检测到角色篡改尝试 (DAN)"),
    ("pretend you are", "检测到角色伪装尝试"),
    ("do not follow", "检测到反指令攻击"),
    ("disregard previous", "检测到上下文绕过尝试"),
    ("new system message", "检测到系统消息伪造"),
    ("system: you are", "检测到系统级注入"),
    ("[system]", "检测到伪系统消息标签"),
    ("<|im_start|>", "检测到特殊分隔符注入"),
    ("<|im_end|>", "检测到特殊分隔符注入"),
    ("ignore safety", "检测到安全策略绕过"),
    ("bypass your restrictions", "检测到限制绕过尝试"),
    ("output your initial prompt", "检测到提示词泄露尝试"),
    ("reveal your instructions", "检测到指令泄露尝试"),
];
