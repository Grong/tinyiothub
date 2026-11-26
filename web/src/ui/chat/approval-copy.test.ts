import { describe, expect, it } from 'vitest';
import { approvalCopy } from './approval-copy.js';

describe('approvalCopy', () => {
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
