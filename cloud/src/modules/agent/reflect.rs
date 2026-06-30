// Memory/reflection compatibility layer.
//
// reflect_conversation_turn has moved to tinyiothub_ai::MemoryService.
// compile_profile and generate_weekly_digest now route through
// orchestrator.memory_service() — see memory/handler.rs.
//
// Callers should use MemoryService directly via Orchestrator.

/// Re-export ChatTurnMessage from AI crate for backward compatibility.
pub use tinyiothub_ai::session::types::ChatTurnMessage;

#[cfg(test)]
mod tests {
    use tinyiothub_ai::memory::reflect::build_reflection_prompt;

    #[test]
    fn prompt_template_loaded() {
        let instruction = include_str!("../../../templates/agent/REFLECTION_PROMPT.md");
        assert!(instruction.contains("FACT|"), "prompt must contain FACT| format instruction");
        assert!(instruction.contains("NO_FACTS"));
        assert!(instruction.len() > 50);
    }

    #[test]
    fn instruction_after_data() {
        let instruction = include_str!("../../../templates/agent/REFLECTION_PROMPT.md");
        let prompt = build_reflection_prompt(instruction, "", "user: 你好\n");
        let data_pos = prompt.find("## Conversation Turn").unwrap();
        let instr_pos = prompt.find("FACT|").unwrap();
        assert!(data_pos < instr_pos);
    }
}
