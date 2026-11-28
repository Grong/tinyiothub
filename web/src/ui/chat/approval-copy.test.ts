import { describe, expect, it } from 'vitest';
import { approvalCopy } from './approval-copy.js';

describe('approvalCopy', () => {
  it('turns invoke_action actionName and params into a Chinese device action', () => {
    const copy = approvalCopy({
      tool: 'invoke_action',
      summary: JSON.stringify({
        actionName: 'set_brightness',
        thingId: 'device-light-warehouse',
        params: { brightness: 70, confirmationToken: 'secret-token' },
      }),
    });
    expect(copy.title).toBe('调整亮度');
    expect(copy.description).toContain('动作：调整亮度（set_brightness）');
    expect(copy.description).toContain('亮度');
    expect(JSON.stringify(copy)).not.toContain('secret-token');
  });

  it('turns call_device JSON into a Chinese device action', () => {
    const copy = approvalCopy({
      tool: 'call_device',
      summary: JSON.stringify({
        action: 'turn_off',
        thingId: 'device-light-workshop',
        params: { confirmationToken: 'secret-token' },
      }),
    });
    expect(copy.title).toBe('关闭设备');
    expect(copy.description).toContain('设备：device-light-workshop');
    expect(copy.description).toContain('动作：关闭');
    expect(JSON.stringify(copy)).not.toContain('secret-token');
  });
});
