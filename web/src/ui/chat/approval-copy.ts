/** User-facing copy for an approval, including legacy payloads without title/description. */
export interface ApprovalCopyInput {
  tool: string;
  title?: string;
  description?: string;
  summary: string;
}

const ACTION_LABELS: Record<string, string> = {
  turn_on: '开启设备',
  turn_off: '关闭设备',
  reboot: '重启设备',
  reconnect: '重连设备',
  lock: '上锁',
  unlock: '解锁',
  set_brightness: '调整亮度',
  open: '打开',
  close: '关闭',
};

const TOOL_LABELS: Record<string, string> = {
  call_device: '执行设备操作',
  invoke_action: '执行设备操作',
  send_command: '执行设备命令',
  set_device: '调整设备属性',
  write_properties: '调整设备属性',
};

function parseArgs(summary: string): Record<string, unknown> | null {
  try {
    const value = JSON.parse(summary);
    return value && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function sanitized(summary: string): string {
  const args = parseArgs(summary);
  if (!args) return summary;
  const clean = { ...args };
  delete clean.confirmationToken;
  delete clean.confirmation_token;
  return JSON.stringify(clean, null, 2);
}

export function approvalCopy(input: ApprovalCopyInput): { title: string; description: string } {
  const args = parseArgs(input.summary);
  const rawAction = typeof args?.action === 'string'
    ? args.action
    : typeof args?.actionName === 'string'
      ? args.actionName
      : '';
  const title = input.title
    || ACTION_LABELS[rawAction]
    || TOOL_LABELS[input.tool]
    || '设备操作需要批准';
  if (input.description) return { title, description: input.description };
  const thingId = typeof args?.thingId === 'string' ? args.thingId : '';
  if (rawAction || thingId) {
    const label = ACTION_LABELS[rawAction]?.replace('设备', '') || rawAction || '未提供';
    const params = args?.params && typeof args.params === 'object' && !Array.isArray(args.params)
      ? { ...(args.params as Record<string, unknown>) }
      : null;
    if (params) {
      delete params.confirmationToken;
      delete params.confirmation_token;
    }
    return {
      title,
      description: [
        thingId ? `设备：${thingId}` : '',
        `动作：${label}（${rawAction || '未提供'}）`,
        params && Object.keys(params).length > 0 ? `参数：${JSON.stringify(params)}` : '',
      ]
        .filter(Boolean)
        .join('\n'),
    };
  }
  return { title, description: sanitized(input.summary) };
}
