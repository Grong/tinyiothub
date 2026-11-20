/**
 * Landing-page FAQ — the visible answers and the FAQPage structured data share
 * this single source so search/AI crawlers never see a different claim than
 * users do.
 */
export interface HomeFaqItem {
  question: string;
  answer: string;
}

export const HOME_FAQ: HomeFaqItem[] = [
  {
    question: 'TinyIoTHub 是什么？',
    answer:
      'TinyIoTHub 是一个开源的 IoT agent harness（物联网智能体运行环境）。它把设备、空间、产线、楼宇统一建模为层级化的「物」，让一个常驻 Assistant 被设备事件和定时任务唤醒，查询物模型、做出判断、通过审批后执行动作，并回读属性验证结果。服务端使用 Rust + SQLite，单进程即可私有化部署，MIT 协议开源。',
  },
  {
    question: '它和 ThingsBoard 这类 IoT 平台有什么区别？',
    answer:
      '传统 IoT 平台的重心是设备接入、数据看板和规则告警；TinyIoTHub 把设备接入当作底座，产品本体是设备之上的 harness 层：统一 Session Loop、SKILL.md 工作手册、待办管理、执行中提问、子代理上下文隔离，以及面向物理世界的安全执行。两者并不冲突，TinyIoTHub 可以通过 MQTT 接入现有网关。',
  },
  {
    question: '和 Claude Code、LangGraph 等通用 agent 框架有什么不同？',
    answer:
      '通用 agent 框架主要管理比特流，TinyIoTHub 管理的是物理动作。设备动作不可重试，所以每个动作都要记录「意图 → 派发 → 确认 → 未知」四种状态，未知状态停下来对账，绝不自动重放；Assistant 由设备事件和定时 tick 唤醒，依据物模型与知识文档判断，执行后回读属性，并把全过程写入审计账本。',
  },
  {
    question: '支持哪些设备协议？现在就能用吗？',
    answer:
      'MQTT 已可用于设备接入与边缘网关上报；Modbus、ONVIF、SNMP 的驱动框架与 plugin-sdk 已就绪，具体驱动仍在开发中。平台同时支持 DTDL / WoT 物模板导入导出，以及内嵌 MCP Server（Claude Desktop、Cursor 可直接连接）。',
  },
  {
    question: '部署和资源占用是什么水平？',
    answer:
      '单个 Rust 二进制加内置 SQLite，不依赖外部数据库；官方提供 linux/amd64 与 linux/arm64 多架构 Docker 镜像。单进程约 80MB 内存即可运行，适合现场机房与边缘盒子，也支持完全私有化。',
  },
  {
    question: '新增一个巡检场景需要改代码吗？',
    answer:
      '不需要改 Rust。场景写成 SKILL.md 工作手册放入技能目录，下一轮 Assistant 就会按手册执行。官方已交付巡检、故障诊断、心跳监控、告警管理等 10 个技能，也支持用户自写技能；规划中的应急预案、节能策略、视频分析会明确标注为未交付。',
  },
];

export function homeFaqJsonLd(): string {
  return JSON.stringify({
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: HOME_FAQ.map(({ question, answer }) => ({
      '@type': 'Question',
      name: question,
      acceptedAnswer: { '@type': 'Answer', text: answer },
    })),
  });
}
