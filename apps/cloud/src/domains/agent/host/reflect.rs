// 数据实现，留 cloud（D2）
// Memory/reflection compatibility layer.
//
// reflect_conversation_turn lives in tinyiothub_memory::service::MemoryService.
// compile_profile and generate_weekly_digest route through
// AgentState.memory_service (cloud-held since Task 6) — see memory/handler.rs.
//
// Callers should use MemoryService directly via AgentState.

/// Re-export ChatTurnMessage from AI crate for backward compatibility.
pub use tinyiothub_llm::session::types::ChatTurnMessage;

#[cfg(test)]
mod tests {
    use tinyiothub_agent::memory::reflect::build_reflection_prompt;

    #[test]
    fn prompt_template_loaded() {
        let instruction = tinyiothub_agent::prompt::templates::REFLECTION_PROMPT_MD;
        assert!(
            instruction.contains("FACT|"),
            "prompt must contain FACT| format instruction"
        );
        assert!(instruction.contains("NO_FACTS"));
        assert!(instruction.len() > 50);
    }

    #[test]
    fn instruction_after_data() {
        let instruction = tinyiothub_agent::prompt::templates::REFLECTION_PROMPT_MD;
        let prompt = build_reflection_prompt(instruction, "", "user: 你好\n");
        let data_pos = prompt.find("## Conversation Turn").unwrap();
        let instr_pos = prompt.find("FACT|").unwrap();
        assert!(data_pos < instr_pos);
    }
}
