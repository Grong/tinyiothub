//! Alias-bound attribution surface used by every emission in the
//! workspace. Each "thing" that participates in an event (channel,
//! agent, tool, cron job, model provider, memory backend, peer group,
//! skill bundle, MCP bundle, session) implements [`Attributable`].
//! Entry points open `attribution_span!(thing)` once at the start of
//! their work; the `LogCaptureLayer` in `zeroclaw-log` walks the span
//! scope and fills the typed attribution slots automatically.
//!
//! Adding a new variant: extend the relevant `Kind` enum (the variant
//! name's snake_case form is the canonical `<type>` string), and — only
//! if a new role family is needed — update the [`Role::composite_prefix`]
//! / [`Role::attribution_field`] / [`Role::default_category`] match arms.
//! No call-site changes.
//!
//! Vendored 自 zeroclaw-api/src/attribution.rs。原文件用
//! `strum::IntoStaticStr` 生成 `From<Kind> for &'static str`；
//! 本 crate 不引 strum，各 Kind 的手写 match 实现与 strum 的
//! snake_case 序列化逐 variant 等价。

/// Trait every alias-bound "thing" implements once next to its struct.
pub trait Attributable {
    fn role(&self) -> Role;
    fn alias(&self) -> &str;
}

impl<T: Attributable + ?Sized> Attributable for std::sync::Arc<T> {
    fn role(&self) -> Role {
        (**self).role()
    }
    fn alias(&self) -> &str {
        (**self).alias()
    }
}

impl<T: Attributable + ?Sized> Attributable for Box<T> {
    fn role(&self) -> Role {
        (**self).role()
    }
    fn alias(&self) -> &str {
        (**self).alias()
    }
}

impl<T: Attributable + ?Sized> Attributable for &T {
    fn role(&self) -> Role {
        (**self).role()
    }
    fn alias(&self) -> &str {
        (**self).alias()
    }
}

/// Closed taxonomy of every role a thing can fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Swarm,
    Agent,
    Channel(ChannelKind),
    Tool(ToolKind),
    Cron(CronKind),
    Provider(ProviderKind),
    Memory(MemoryKind),
    PeerGroup,
    Skill,
    Mcp,
    Sop,
    Session,
    System,
}

/// Channel implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    AcpChannel,
    Amqp,
    Bluesky,
    ClawdTalk,
    Cli,
    DingTalk,
    Discord,
    Email,
    GmailPush,
    IMessage,
    Irc,
    Lark,
    Line,
    Linq,
    Matrix,
    Mattermost,
    MoChat,
    NextcloudTalk,
    Nostr,
    Notion,
    Qq,
    Reddit,
    Signal,
    Slack,
    Telegram,
    Twitch,
    Twitter,
    VoiceCall,
    VoiceWake,
    Wati,
    WeCom,
    WeComWs,
    Webhook,
    Wechat,
    WhatsappBusiness,
    WhatsappWeb,
}

impl From<ChannelKind> for &'static str {
    fn from(k: ChannelKind) -> Self {
        match k {
            ChannelKind::AcpChannel => "acp",
            ChannelKind::Amqp => "amqp",
            ChannelKind::Bluesky => "bluesky",
            ChannelKind::ClawdTalk => "clawdtalk",
            ChannelKind::Cli => "cli",
            ChannelKind::DingTalk => "dingtalk",
            ChannelKind::Discord => "discord",
            ChannelKind::Email => "email",
            ChannelKind::GmailPush => "gmail_push",
            ChannelKind::IMessage => "imessage",
            ChannelKind::Irc => "irc",
            ChannelKind::Lark => "lark",
            ChannelKind::Line => "line",
            ChannelKind::Linq => "linq",
            ChannelKind::Matrix => "matrix",
            ChannelKind::Mattermost => "mattermost",
            ChannelKind::MoChat => "mochat",
            ChannelKind::NextcloudTalk => "nextcloud_talk",
            ChannelKind::Nostr => "nostr",
            ChannelKind::Notion => "notion",
            ChannelKind::Qq => "qq",
            ChannelKind::Reddit => "reddit",
            ChannelKind::Signal => "signal",
            ChannelKind::Slack => "slack",
            ChannelKind::Telegram => "telegram",
            ChannelKind::Twitch => "twitch",
            ChannelKind::Twitter => "twitter",
            ChannelKind::VoiceCall => "voice_call",
            ChannelKind::VoiceWake => "voice_wake",
            ChannelKind::Wati => "wati",
            ChannelKind::WeCom => "wecom",
            ChannelKind::WeComWs => "wecom_ws",
            ChannelKind::Webhook => "webhook",
            ChannelKind::Wechat => "wechat",
            ChannelKind::WhatsappBusiness => "whatsapp_business",
            ChannelKind::WhatsappWeb => "whatsapp_web",
        }
    }
}

/// Built-in tool implementations. Closed set — plugins that need their
/// own attribution add a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Shell,
    HttpRequest,
    HttpServer,
    FetchUrl,
    Search,
    Memory,
    SpawnSubagent,
    SopList,
    SopExecute,
    SopApprove,
    SopAdvance,
    SopStatus,
    SopHistory,
    Wait,
    Plugin,
}

impl From<ToolKind> for &'static str {
    fn from(k: ToolKind) -> Self {
        match k {
            ToolKind::Shell => "shell",
            ToolKind::HttpRequest => "http_request",
            ToolKind::HttpServer => "http_server",
            ToolKind::FetchUrl => "fetch_url",
            ToolKind::Search => "search",
            ToolKind::Memory => "memory",
            ToolKind::SpawnSubagent => "spawn_subagent",
            ToolKind::SopList => "sop_list",
            ToolKind::SopExecute => "sop_execute",
            ToolKind::SopApprove => "sop_approve",
            ToolKind::SopAdvance => "sop_advance",
            ToolKind::SopStatus => "sop_status",
            ToolKind::SopHistory => "sop_history",
            ToolKind::Wait => "wait",
            ToolKind::Plugin => "plugin",
        }
    }
}

/// Cron schedule shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronKind {
    Interval,
    At,
    Cron,
    Once,
}

impl From<CronKind> for &'static str {
    fn from(k: CronKind) -> Self {
        match k {
            CronKind::Interval => "interval",
            CronKind::At => "at",
            CronKind::Cron => "cron",
            CronKind::Once => "once",
        }
    }
}

/// Provider family. The inner enum carries the specific implementation;
/// the outer family drives which composite prefix (`model_provider` /
/// `tts_provider` / …) the layer populates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Model(ModelProviderKind),
    Tts(TtsProviderKind),
    Transcription(TranscriptionProviderKind),
    Tunnel(TunnelProviderKind),
}

impl ProviderKind {
    #[must_use]
    pub fn type_str(self) -> &'static str {
        match self {
            Self::Model(k) => k.into(),
            Self::Tts(k) => k.into(),
            Self::Transcription(k) => k.into(),
            Self::Tunnel(k) => k.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProviderKind {
    Anthropic,
    OpenAi,
    OpenAiCodex,
    Azure,
    Together,
    Bedrock,
    Ollama,
    Gemini,
    GeminiCli,
    GoogleAi,
    Mistral,
    Groq,
    OpenRouter,
    Telnyx,
    Copilot,
    Glm,
    KiloCli,
    Kilo,
    Router,
    Moonshot,
    Qwen,
    Minimax,
    Zai,
    Doubao,
    Yi,
    Hunyuan,
    Qianfan,
    Baichuan,
    Fireworks,
    Deepseek,
    AtomicChat,
    Cohere,
    Perplexity,
    Xai,
    Cerebras,
    Sambanova,
    Hyperbolic,
    Deepinfra,
    Huggingface,
    Ai21,
    Reka,
    Baseten,
    Nscale,
    Anyscale,
    Nebius,
    Friendli,
    Stepfun,
    Aihubmix,
    Siliconflow,
    Astrai,
    Avian,
    Deepmyst,
    Venice,
    Nearai,
    Novita,
    Nvidia,
    Vercel,
    Cloudflare,
    Ovh,
    Lmstudio,
    Llamacpp,
    Sglang,
    Vllm,
    Osaurus,
    Litellm,
    Lepton,
    Synthetic,
    Opencode,
    Custom,
    Plugin,
}

impl From<ModelProviderKind> for &'static str {
    fn from(k: ModelProviderKind) -> Self {
        match k {
            ModelProviderKind::Anthropic => "anthropic",
            ModelProviderKind::OpenAi => "openai",
            ModelProviderKind::OpenAiCodex => "openai_codex",
            ModelProviderKind::Azure => "azure",
            ModelProviderKind::Together => "together",
            ModelProviderKind::Bedrock => "bedrock",
            ModelProviderKind::Ollama => "ollama",
            ModelProviderKind::Gemini => "gemini",
            ModelProviderKind::GeminiCli => "gemini_cli",
            ModelProviderKind::GoogleAi => "google_ai",
            ModelProviderKind::Mistral => "mistral",
            ModelProviderKind::Groq => "groq",
            ModelProviderKind::OpenRouter => "open_router",
            ModelProviderKind::Telnyx => "telnyx",
            ModelProviderKind::Copilot => "copilot",
            ModelProviderKind::Glm => "glm",
            ModelProviderKind::KiloCli => "kilo_cli",
            ModelProviderKind::Kilo => "kilo",
            ModelProviderKind::Router => "router",
            ModelProviderKind::Moonshot => "moonshot",
            ModelProviderKind::Qwen => "qwen",
            ModelProviderKind::Minimax => "minimax",
            ModelProviderKind::Zai => "zai",
            ModelProviderKind::Doubao => "doubao",
            ModelProviderKind::Yi => "yi",
            ModelProviderKind::Hunyuan => "hunyuan",
            ModelProviderKind::Qianfan => "qianfan",
            ModelProviderKind::Baichuan => "baichuan",
            ModelProviderKind::Fireworks => "fireworks",
            ModelProviderKind::Deepseek => "deepseek",
            ModelProviderKind::AtomicChat => "atomic_chat",
            ModelProviderKind::Cohere => "cohere",
            ModelProviderKind::Perplexity => "perplexity",
            ModelProviderKind::Xai => "xai",
            ModelProviderKind::Cerebras => "cerebras",
            ModelProviderKind::Sambanova => "sambanova",
            ModelProviderKind::Hyperbolic => "hyperbolic",
            ModelProviderKind::Deepinfra => "deepinfra",
            ModelProviderKind::Huggingface => "huggingface",
            ModelProviderKind::Ai21 => "ai21",
            ModelProviderKind::Reka => "reka",
            ModelProviderKind::Baseten => "baseten",
            ModelProviderKind::Nscale => "nscale",
            ModelProviderKind::Anyscale => "anyscale",
            ModelProviderKind::Nebius => "nebius",
            ModelProviderKind::Friendli => "friendli",
            ModelProviderKind::Stepfun => "stepfun",
            ModelProviderKind::Aihubmix => "aihubmix",
            ModelProviderKind::Siliconflow => "siliconflow",
            ModelProviderKind::Astrai => "astrai",
            ModelProviderKind::Avian => "avian",
            ModelProviderKind::Deepmyst => "deepmyst",
            ModelProviderKind::Venice => "venice",
            ModelProviderKind::Nearai => "nearai",
            ModelProviderKind::Novita => "novita",
            ModelProviderKind::Nvidia => "nvidia",
            ModelProviderKind::Vercel => "vercel",
            ModelProviderKind::Cloudflare => "cloudflare",
            ModelProviderKind::Ovh => "ovh",
            ModelProviderKind::Lmstudio => "lmstudio",
            ModelProviderKind::Llamacpp => "llamacpp",
            ModelProviderKind::Sglang => "sglang",
            ModelProviderKind::Vllm => "vllm",
            ModelProviderKind::Osaurus => "osaurus",
            ModelProviderKind::Litellm => "litellm",
            ModelProviderKind::Lepton => "lepton",
            ModelProviderKind::Synthetic => "synthetic",
            ModelProviderKind::Opencode => "opencode",
            ModelProviderKind::Custom => "custom",
            ModelProviderKind::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsProviderKind {
    OpenAi,
    ElevenLabs,
    Cartesia,
    Google,
    Edge,
    Piper,
    Plugin,
}

impl From<TtsProviderKind> for &'static str {
    fn from(k: TtsProviderKind) -> Self {
        match k {
            TtsProviderKind::OpenAi => "openai",
            TtsProviderKind::ElevenLabs => "elevenlabs",
            TtsProviderKind::Cartesia => "cartesia",
            TtsProviderKind::Google => "google",
            TtsProviderKind::Edge => "edge",
            TtsProviderKind::Piper => "piper",
            TtsProviderKind::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionProviderKind {
    Whisper,
    OpenAi,
    Deepgram,
    Groq,
    AssemblyAi,
    Google,
    Plugin,
}

impl From<TranscriptionProviderKind> for &'static str {
    fn from(k: TranscriptionProviderKind) -> Self {
        match k {
            TranscriptionProviderKind::Whisper => "whisper",
            TranscriptionProviderKind::OpenAi => "openai",
            TranscriptionProviderKind::Deepgram => "deepgram",
            TranscriptionProviderKind::Groq => "groq",
            TranscriptionProviderKind::AssemblyAi => "assembly_ai",
            TranscriptionProviderKind::Google => "google",
            TranscriptionProviderKind::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelProviderKind {
    Ngrok,
    Cloudflared,
    OpenVpn,
    Pinggy,
    Tailscale,
    None,
    Custom,
    Plugin,
}

impl From<TunnelProviderKind> for &'static str {
    fn from(k: TunnelProviderKind) -> Self {
        match k {
            TunnelProviderKind::Ngrok => "ngrok",
            TunnelProviderKind::Cloudflared => "cloudflared",
            TunnelProviderKind::OpenVpn => "open_vpn",
            TunnelProviderKind::Pinggy => "pinggy",
            TunnelProviderKind::Tailscale => "tailscale",
            TunnelProviderKind::None => "none",
            TunnelProviderKind::Custom => "custom",
            TunnelProviderKind::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Sqlite,
    Json,
    InMemory,
    Markdown,
    AgentScopedMarkdown,
    AgentScoped,
    Qdrant,
    Postgres,
    Lucid,
    None,
    Plugin,
}

impl From<MemoryKind> for &'static str {
    fn from(k: MemoryKind) -> Self {
        match k {
            MemoryKind::Sqlite => "sqlite",
            MemoryKind::Json => "json",
            MemoryKind::InMemory => "in_memory",
            MemoryKind::Markdown => "markdown",
            MemoryKind::AgentScopedMarkdown => "agent_scoped_markdown",
            MemoryKind::AgentScoped => "agent_scoped",
            MemoryKind::Qdrant => "qdrant",
            MemoryKind::Postgres => "postgres",
            MemoryKind::Lucid => "lucid",
            MemoryKind::None => "none",
            MemoryKind::Plugin => "plugin",
        }
    }
}

impl Role {
    /// Composite prefix this role populates (`channel`, `model_provider`,
    /// `tts_provider`, `transcription_provider`, `tunnel_provider`),
    /// or `None` for roles that use a plain attribution field.
    #[must_use]
    pub fn composite_prefix(self) -> Option<&'static str> {
        match self {
            Self::Channel(_) => Some("channel"),
            Self::Provider(ProviderKind::Model(_)) => Some("model_provider"),
            Self::Provider(ProviderKind::Tts(_)) => Some("tts_provider"),
            Self::Provider(ProviderKind::Transcription(_)) => Some("transcription_provider"),
            Self::Provider(ProviderKind::Tunnel(_)) => Some("tunnel_provider"),
            _ => None,
        }
    }

    /// The `<type>` portion of the composite, when this role contributes
    /// to one.
    #[must_use]
    pub fn composite_type(self) -> Option<&'static str> {
        match self {
            Self::Channel(k) => Some(k.into()),
            Self::Provider(p) => Some(p.type_str()),
            _ => None,
        }
    }

    /// Plain-attribution-field key this role populates for roles that
    /// don't use a composite. `Tool` writes `tool`; `Agent` writes
    /// `agent_alias`; `Cron` writes `cron_job_id`; …
    #[must_use]
    pub fn attribution_field(self) -> Option<&'static str> {
        match self {
            Self::Agent => Some("agent_alias"),
            Self::Tool(_) => Some("tool"),
            Self::Cron(_) => Some("cron_job_id"),
            Self::Memory(_) => Some("memory_namespace"),
            Self::PeerGroup => Some("peer_group"),
            Self::Skill => Some("skill_bundle"),
            Self::Mcp => Some("mcp_bundle"),
            Self::Sop => Some("sop_name"),
            Self::Session => Some("session_key"),
            _ => None,
        }
    }

    /// Stable string tag used by the span layer to identify the role's
    /// family. The inner Kind (when applicable) is rendered alongside in
    /// [`Role::composite_type`].
    #[must_use]
    pub fn family_str(self) -> &'static str {
        match self {
            Self::Swarm => "swarm",
            Self::Agent => "agent",
            Self::Channel(_) => "channel",
            Self::Tool(_) => "tool",
            Self::Cron(_) => "cron",
            Self::Provider(ProviderKind::Model(_)) => "provider.model",
            Self::Provider(ProviderKind::Tts(_)) => "provider.tts",
            Self::Provider(ProviderKind::Transcription(_)) => "provider.transcription",
            Self::Provider(ProviderKind::Tunnel(_)) => "provider.tunnel",
            Self::Memory(_) => "memory",
            Self::PeerGroup => "peer_group",
            Self::Skill => "skill",
            Self::Mcp => "mcp",
            Self::Sop => "sop",
            Self::Session => "session",
            Self::System => "system",
        }
    }

    /// Closest `zeroclaw_log::EventCategory` for this role, used by
    /// the layer to default `event.category` when the call site doesn't
    /// override. Returned as a `&'static str` to keep `zeroclaw-api`
    /// free of a back-dep on `zeroclaw-log`.
    #[must_use]
    pub fn default_category(self) -> &'static str {
        match self {
            Self::Swarm | Self::Agent => "agent",
            Self::Channel(_) => "channel",
            Self::Tool(_) => "tool",
            Self::Cron(_) => "cron",
            Self::Provider(ProviderKind::Model(_)) => "model_provider",
            Self::Provider(ProviderKind::Tts(_)) => "tts_provider",
            Self::Provider(ProviderKind::Transcription(_)) => "transcription_provider",
            Self::Provider(ProviderKind::Tunnel(_)) => "tunnel_provider",
            Self::Memory(_) => "memory",
            Self::Session => "session",
            Self::Sop => "sop",
            Self::PeerGroup | Self::Skill | Self::Mcp | Self::System => "system",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kind_snake_case() {
        assert_eq!(<&'static str>::from(ChannelKind::Telegram), "telegram");
        assert_eq!(
            <&'static str>::from(ChannelKind::WhatsappBusiness),
            "whatsapp_business"
        );
    }

    #[test]
    fn provider_kind_delegates_to_inner() {
        assert_eq!(
            ProviderKind::Model(ModelProviderKind::Anthropic).type_str(),
            "anthropic"
        );
        assert_eq!(
            ProviderKind::Tts(TtsProviderKind::ElevenLabs).type_str(),
            "elevenlabs"
        );
    }

    #[test]
    fn role_composite_prefix() {
        assert_eq!(
            Role::Channel(ChannelKind::Discord).composite_prefix(),
            Some("channel")
        );
        assert_eq!(
            Role::Provider(ProviderKind::Model(ModelProviderKind::Anthropic)).composite_prefix(),
            Some("model_provider"),
        );
        assert!(Role::Agent.composite_prefix().is_none());
    }

    #[test]
    fn role_attribution_field() {
        assert_eq!(Role::Agent.attribution_field(), Some("agent_alias"));
        assert_eq!(
            Role::Tool(ToolKind::Shell).attribution_field(),
            Some("tool")
        );
        assert!(
            Role::Channel(ChannelKind::Telegram)
                .attribution_field()
                .is_none()
        );
    }
}
