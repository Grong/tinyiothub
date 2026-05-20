use tinyiothub_core::memory::AgentMemory;

/// Check if assistant text references a memory. Lightweight sliding-window probe.
/// No LLM needed — this is a statistical signal, not a semantic judgment.
///
/// Splits the memory content into tokens (words split on whitespace/punctuation,
/// plus the CJK particle "的"), then slides windows of up to 8 tokens joined
/// with spaces through the assistant text.
///
/// Length guard adapts to content length:
/// - Content >= 20 chars: minimum probe of 20 chars (safe for CJK mixed text)
/// - Content < 20 chars: minimum probe of 6 chars (handles short CJK phrases
///   where single tokens carry more semantic weight)
pub fn check_reference(memory: &AgentMemory, assistant_text: &str) -> bool {
    let words: Vec<&str> = memory
        .content
        .split(|c: char| {
            c.is_whitespace()
                || c.is_ascii_punctuation()
                || c == '，'
                || c == '。'
                || c == '的'
        })
        .filter(|s| !s.is_empty())
        .collect();
    if words.is_empty() {
        return false;
    }

    let content_len = memory.content.chars().count();
    let min_probe_len: usize = if content_len >= 20 { 20 } else { 6 };

    let max_win = words.len().min(8);
    for size in 1..=max_win {
        for window in words.windows(size) {
            let probe = window.join(" ");
            if probe.chars().count() >= min_probe_len && assistant_text.contains(&probe) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory(content: &str) -> AgentMemory {
        AgentMemory {
            id: "test".into(),
            workspace_id: "ws".into(),
            agent_id: "a".into(),
            zone: tinyiothub_core::memory::MemoryZone::Core,
            content: content.into(),
            source: tinyiothub_core::memory::MemorySource::User,
            confidence: tinyiothub_core::memory::Confidence::High,
            tags: vec![],
            pinned: false,
            supersedes: None,
            device_id: None,
            snapshot_data: None,
            snapshot_time: None,
            effectiveness: 1.0,
            load_count: 0,
            reference_count: 0,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn detects_english_reference() {
        let mem = make_memory("User manages Building A campus with 8 buildings total");
        let text = "Based on what you mentioned about managing Building A campus with 8 buildings...";
        assert!(check_reference(&mem, text));
    }

    #[test]
    fn no_false_positive_on_short_probe() {
        let mem = make_memory("OK");
        let text = "OK, I'll do that";
        assert!(!check_reference(&mem, text));
    }

    #[test]
    fn detects_chinese_reference() {
        let mem = make_memory("用户管理上海园区的智能楼宇系统");
        let text = "根据你之前提到的上海园区智能楼宇系统，我建议...";
        assert!(check_reference(&mem, text));
    }

    #[test]
    fn no_match_on_unrelated_text() {
        let mem = make_memory("The HVAC system in Building 3 is running hot");
        let text = "I'll help you configure the Modbus driver for your new device";
        assert!(!check_reference(&mem, text));
    }
}
