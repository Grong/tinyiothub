/**
 * evidence-segments —— 证据原文切分（AI 运行透明化 T13 抽取）。
 *
 * 从 dispositions.ts 原样搬到 ui/shared/：T13 run 详情页也要切 think/verdict，
 * 而 dispositions.ts 模块级耦合卡片视图（sse-client/judgments/toast 副作用），
 * 详情页不应为两个纯函数引入整张卡片。dispositions.ts 保留 re-export，
 * 既有导入（含 T12 测试）不受影响。
 *
 * S3：切分产物一律走文本绑定渲染，禁 md()/innerHTML。
 */

/** 证据段落：think = 思考块，text = 正文，verdict = 尾部协议块原文。 */
export interface EvidenceSegment {
  kind: "think" | "text" | "verdict";
  text: string;
}

/** 证据原文分段。完整记录不丢弃：think 块与 verdict 协议块
 *  各自成段（渲染层折叠展示），正文按原顺序保留。 */
export function segmentEvidence(raw: string): EvidenceSegment[] {
  // 先摘尾部 verdict 协议块（调查指令约定在末尾；完整保留，折叠展示）
  let body = raw;
  let verdict: string | null = null;
  const fenceStart = body.lastIndexOf("```json");
  if (fenceStart >= 0) {
    const fenceEnd = body.indexOf("```", fenceStart + 7);
    if (fenceEnd >= 0) {
      verdict = body.slice(fenceStart + 7, fenceEnd).trim() || null;
      body = (body.slice(0, fenceStart) + body.slice(fenceEnd + 3)).trim();
    }
  }
  const segments: EvidenceSegment[] = [];
  const thinkRe = /<think>([\s\S]*?)(?:<\/think>|$)/g; // 兼容未闭合尾块
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = thinkRe.exec(body)) !== null) {
    const before = body.slice(last, m.index).trim();
    if (before) segments.push({ kind: "text", text: before });
    const think = m[1].trim();
    if (think) segments.push({ kind: "think", text: think });
    last = m.index + m[0].length;
  }
  const tail = body.slice(last).trim();
  if (tail) segments.push({ kind: "text", text: tail });
  if (verdict) segments.push({ kind: "verdict", text: verdict });
  return segments;
}

/** 证据原文提取：新行读 summary（完整记录），存量老行读 excerpt。 */
export function evidenceRawText(evidence: unknown): string {
  if (!evidence || typeof evidence !== "object") return "";
  const ev = evidence as { summary?: unknown; excerpt?: unknown };
  if (typeof ev.summary === "string") return ev.summary;
  if (typeof ev.excerpt === "string") return ev.excerpt;
  return "";
}
