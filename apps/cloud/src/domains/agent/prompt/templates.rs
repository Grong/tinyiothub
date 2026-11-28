//! Agent prompt 模板 — 编译期内嵌（`include_str!`），单一家：
//! `apps/cloud/templates/agent/`。
//!
//! 消费方：本 crate 的 prompt 组装（内嵌兜底层），以及组合层的 workspace
//! scaffold / files API / reflection（经这些常量引用，禁止跨 crate 相对路径
//! include）。

/// ASSISTANT.md — Assistant 默认职责与每轮工作方式
pub const ASSISTANT_MD: &str = include_str!("../../../../templates/agent/ASSISTANT.md");
/// VISUALIZATION.md — 可视化技能模板
pub const VISUALIZATION_SKILL_MD: &str = include_str!("../../../../templates/skills/visualization.md");
/// INSPECTION.md — 巡检技能模板
pub const INSPECTION_SKILL_MD: &str = include_str!("../../../../templates/skills/inspection.md");
/// VIDEO_INSPECTION.md — 视频巡检技能模板
pub const VIDEO_INSPECTION_SKILL_MD: &str = include_str!("../../../../templates/skills/video-inspection.md");

/// 产品技能模板（文件名 stem, 正文）。`apps/cloud/templates/skills/` 是唯一真源，
/// 启动时同步到全局技能目录；技能正文里的工具名必须属于
/// `tools::catalog::TOOL_CATALOG` 的规范名或 `PLATFORM_PLANE_TOOLS`。
pub const SKILL_TEMPLATES: &[(&str, &str)] = &[
    (
        "alarm-management",
        include_str!("../../../../templates/skills/alarm-management.md"),
    ),
    (
        "device-management",
        include_str!("../../../../templates/skills/device-management.md"),
    ),
    (
        "driver-management",
        include_str!("../../../../templates/skills/driver-management.md"),
    ),
    (
        "energy-saving",
        include_str!("../../../../templates/skills/energy-saving.md"),
    ),
    ("inspection", INSPECTION_SKILL_MD),
    (
        "job-management",
        include_str!("../../../../templates/skills/job-management.md"),
    ),
    (
        "troubleshooting",
        include_str!("../../../../templates/skills/troubleshooting.md"),
    ),
    ("video-inspection", VIDEO_INSPECTION_SKILL_MD),
    ("visualization", VISUALIZATION_SKILL_MD),
    ("workspace", include_str!("../../../../templates/skills/workspace.md")),
];

/// 已下线的内置技能：启动时改名成 `.retired` 归档，避免旧副本继续出现在技能索引里。
pub const RETIRED_SKILLS: &[&str] = &["heartbeat-monitor", "platform-overview"];

/// 内置技能修订号，写在 frontmatter `builtin:` 上。
/// 启动时若工作区副本的修订号更小，用模板覆盖；用户自建技能（非内置文件名）永不触碰。
pub const BUILTIN_SKILL_REV: u32 = 3;

/// IDENTITY.md — 身份层模板
pub const IDENTITY_MD: &str = include_str!("../../../../templates/agent/IDENTITY.md");
/// SOUL.md — 行为原则模板
pub const SOUL_MD: &str = include_str!("../../../../templates/agent/SOUL.md");
/// AGENTS.md — agent 规则模板
pub const AGENTS_MD: &str = include_str!("../../../../templates/agent/AGENTS.md");
/// TOOLS.md — 能力说明模板
pub const TOOLS_MD: &str = include_str!("../../../../templates/agent/TOOLS.md");
/// USER.md — 用户上下文模板
pub const USER_MD: &str = include_str!("../../../../templates/agent/USER.md");
/// MEMORY.md — 长期记忆模板
pub const MEMORY_MD: &str = include_str!("../../../../templates/agent/MEMORY.md");
/// HEARTBEAT.md — 心跳清单模板
pub const HEARTBEAT_MD: &str = include_str!("../../../../templates/agent/HEARTBEAT.md");
/// BOOTSTRAP.md — 首次引导模板
pub const BOOTSTRAP_MD: &str = include_str!("../../../../templates/agent/BOOTSTRAP.md");
/// REFLECTION_PROMPT.md — 反思引擎指令模板
pub const REFLECTION_PROMPT_MD: &str = include_str!("../../../../templates/agent/REFLECTION_PROMPT.md");
/// COMPILE_PROMPT.md — profile 编译指令模板
pub const COMPILE_PROMPT_MD: &str = include_str!("../../../../templates/agent/COMPILE_PROMPT.md");
