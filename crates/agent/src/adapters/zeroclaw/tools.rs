//! port Tool → zeroclaw Tool 桥。

use crate::port::attribution::{self, Attributable};
use crate::port::tool::Tool;

/// port ↔ zeroclaw_api 的 Kind 枚举转换宏。
///
/// 两侧枚举 vendored 自同一源、变体名与顺序一致；宏按变体名清单生成双向 match，
/// 任一侧新增变体即触发非穷尽 match 编译错误（漂移检测）。
/// 仅影响 zeroclaw 日志归属，不进入 LLM 请求。
macro_rules! kind_conv {
    ($to_zc:ident, $to_port:ident, $p:ty, $z:ty, [$($v:ident),+ $(,)?]) => {
        pub(crate) fn $to_zc(k: $p) -> $z {
            match k { $( <$p>::$v => <$z>::$v ),+ }
        }
        #[allow(dead_code)] // Task 5b 的 ZeroclawProviderAsPort 使用
        pub(crate) fn $to_port(k: $z) -> $p {
            match k { $( <$z>::$v => <$p>::$v ),+ }
        }
    };
}

kind_conv!(
    zc_tool_kind,
    port_tool_kind,
    attribution::ToolKind,
    zeroclaw_api::attribution::ToolKind,
    [
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
        Plugin
    ]
);
kind_conv!(
    zc_cron_kind,
    port_cron_kind,
    attribution::CronKind,
    zeroclaw_api::attribution::CronKind,
    [Interval, At, Cron, Once]
);
kind_conv!(
    zc_memory_kind,
    port_memory_kind,
    attribution::MemoryKind,
    zeroclaw_api::attribution::MemoryKind,
    [
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
        Plugin
    ]
);
kind_conv!(
    zc_tts_kind,
    port_tts_kind,
    attribution::TtsProviderKind,
    zeroclaw_api::attribution::TtsProviderKind,
    [OpenAi, ElevenLabs, Cartesia, Google, Edge, Piper, Plugin]
);
kind_conv!(
    zc_transcription_kind,
    port_transcription_kind,
    attribution::TranscriptionProviderKind,
    zeroclaw_api::attribution::TranscriptionProviderKind,
    [Whisper, OpenAi, Deepgram, Groq, AssemblyAi, Google, Plugin]
);
kind_conv!(
    zc_tunnel_kind,
    port_tunnel_kind,
    attribution::TunnelProviderKind,
    zeroclaw_api::attribution::TunnelProviderKind,
    [Ngrok, Cloudflared, OpenVpn, Pinggy, Tailscale, None, Custom, Plugin]
);
kind_conv!(
    zc_channel_kind,
    port_channel_kind,
    attribution::ChannelKind,
    zeroclaw_api::attribution::ChannelKind,
    [
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
        WhatsappWeb
    ]
);
kind_conv!(
    zc_model_kind,
    port_model_kind,
    attribution::ModelProviderKind,
    zeroclaw_api::attribution::ModelProviderKind,
    [
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
        Plugin
    ]
);

pub(crate) fn zc_provider_kind(k: &attribution::ProviderKind) -> zeroclaw_api::attribution::ProviderKind {
    use attribution::ProviderKind as P;
    use zeroclaw_api::attribution::ProviderKind as Z;
    match k {
        P::Model(m) => Z::Model(zc_model_kind(*m)),
        P::Tts(t) => Z::Tts(zc_tts_kind(*t)),
        P::Transcription(t) => Z::Transcription(zc_transcription_kind(*t)),
        P::Tunnel(t) => Z::Tunnel(zc_tunnel_kind(*t)),
    }
}

/// port Attributable → zeroclaw_api Attributable 的 Role 转换。
pub(crate) fn zc_role(role: &attribution::Role) -> zeroclaw_api::attribution::Role {
    use attribution::Role as P;
    use zeroclaw_api::attribution::Role as Z;
    match role {
        P::Swarm => Z::Swarm,
        P::Agent => Z::Agent,
        P::Channel(k) => Z::Channel(zc_channel_kind(*k)),
        P::Tool(k) => Z::Tool(zc_tool_kind(*k)),
        P::Cron(k) => Z::Cron(zc_cron_kind(*k)),
        P::Provider(k) => Z::Provider(zc_provider_kind(k)),
        P::Memory(k) => Z::Memory(zc_memory_kind(*k)),
        P::PeerGroup => Z::PeerGroup,
        P::Skill => Z::Skill,
        P::Mcp => Z::Mcp,
        P::Sop => Z::Sop,
        P::Session => Z::Session,
        P::System => Z::System,
    }
}

/// zeroclaw_api ProviderKind → port ProviderKind（zc_provider_kind 的反向）。
pub(crate) fn port_provider_kind(k: zeroclaw_api::attribution::ProviderKind) -> attribution::ProviderKind {
    use zeroclaw_api::attribution::ProviderKind as Z;
    match k {
        Z::Model(m) => attribution::ProviderKind::Model(port_model_kind(m)),
        Z::Tts(t) => attribution::ProviderKind::Tts(port_tts_kind(t)),
        Z::Transcription(t) => attribution::ProviderKind::Transcription(port_transcription_kind(t)),
        Z::Tunnel(t) => attribution::ProviderKind::Tunnel(port_tunnel_kind(t)),
    }
}

/// zeroclaw_api Attributable → port Attributable 的 Role 转换（zc_role 的反向）。
pub(crate) fn port_role(role: zeroclaw_api::attribution::Role) -> attribution::Role {
    use attribution::Role as P;
    use zeroclaw_api::attribution::Role as Z;
    match role {
        Z::Swarm => P::Swarm,
        Z::Agent => P::Agent,
        Z::Channel(k) => P::Channel(port_channel_kind(k)),
        Z::Tool(k) => P::Tool(port_tool_kind(k)),
        Z::Cron(k) => P::Cron(port_cron_kind(k)),
        Z::Provider(k) => P::Provider(port_provider_kind(k)),
        Z::Memory(k) => P::Memory(port_memory_kind(k)),
        Z::PeerGroup => P::PeerGroup,
        Z::Skill => P::Skill,
        Z::Mcp => P::Mcp,
        Z::Sop => P::Sop,
        Z::Session => P::Session,
        Z::System => P::System,
    }
}

/// port Tool 包装为 zeroclaw Tool（zeroclaw loop 经此调用我们的工具）。
pub struct PortToolAsZeroclaw(pub Box<dyn Tool>);

impl Attributable for PortToolAsZeroclaw {
    fn role(&self) -> attribution::Role {
        self.0.role()
    }
    fn alias(&self) -> &str {
        self.0.alias()
    }
}

impl zeroclaw_api::attribution::Attributable for PortToolAsZeroclaw {
    fn role(&self) -> zeroclaw_api::attribution::Role {
        zc_role(&self.0.role())
    }
    fn alias(&self) -> &str {
        self.0.alias()
    }
}

#[async_trait::async_trait]
impl zeroclaw::tools::Tool for PortToolAsZeroclaw {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn description(&self) -> &str {
        self.0.description()
    }
    fn parameters_schema(&self) -> serde_json::Value {
        self.0.parameters_schema()
    }
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<zeroclaw::tools::ToolResult> {
        let result = self.0.execute(args).await?;
        Ok(zeroclaw::tools::ToolResult {
            success: result.success,
            output: result.output,
            error: result.error,
        })
    }
    fn spec(&self) -> zeroclaw::tools::ToolSpec {
        let spec = self.0.spec();
        zeroclaw::tools::ToolSpec {
            name: spec.name,
            description: spec.description,
            parameters: spec.parameters,
        }
    }
}

/// 便捷：一批 port 工具直接转成 zeroclaw 盒子。
#[allow(dead_code)] // Task 5b 的 zeroclaw_loop_factory 使用
pub(crate) fn wrap_tools(tools: Vec<Box<dyn Tool>>) -> Vec<Box<dyn zeroclaw::tools::Tool>> {
    tools
        .into_iter()
        .map(|t| Box::new(PortToolAsZeroclaw(t)) as Box<dyn zeroclaw::tools::Tool>)
        .collect()
}
