import { describe, expect, it } from 'vitest';
import {
  buildScene,
  groupCards,
  mergeDuplicateLanes,
  type SceneCard,
  type SceneLane,
} from './story-scene.js';
import type { StorySection, StoryStep, StoryView } from './story-model.js';

const VIEW_KEY: Record<string, string> = {
  read_device: 'device.read',
  read_property: 'property.read',
  get_skill: 'skill.load',
  todo_write: 'todo.write',
  complete_skill: 'skill.complete',
  acknowledge_alarm: 'alarm.action',
  alarm_list: 'alarm.list',
  get_thing_profile: 'device.profile',
  query_events: 'event.history',
  set_device: 'device.write',
  call_device: 'device.action',
  spawn_subagent: 'subagent.spawn',
  search_knowledge: 'knowledge.search',
  read_document: 'knowledge.document',
};

function step(overrides: Partial<StoryStep> & Pick<StoryStep, 'id' | 'title'>): StoryStep {
  const viewKey = overrides.viewKey ?? (overrides.tool ? VIEW_KEY[overrides.tool] : undefined);
  return {
    detail: '',
    at: '17:34',
    status: 'done',
    tone: 'read',
    visionIds: [],
    ...overrides,
    viewKey,
  };
}

function section(overrides: Partial<StorySection> = {}): StorySection {
  return {
    id: 'turn-1',
    status: 'done',
    startedAt: '2026-11-27T17:32:00.000Z',
    completedAt: '2026-11-27T17:41:00.000Z',
    steps: [],
    conclusion: '空压站整体正常，3 号机压力已恢复。',
    reasoning: '',
    findingCount: 1,
    retries: 0,
    ...overrides,
  };
}

function story(overrides: Partial<StoryView> = {}): StoryView {
  const first = section();
  return {
    sections: [first],
    prompts: [],
    plans: [],
    devices: [],
    vision: [],
    findings: [],
    stats: { devices: 0, inspected: 0, vision: 0, findings: 0, actions: 0, steps: 0 },
    latestSectionId: first.id,
    latestDevices: [],
    latestVision: [],
    latestFindings: [],
    latestStats: { devices: 0, inspected: 0, vision: 0, findings: 0, actions: 0 },
    ...overrides,
  };
}

describe('buildScene', () => {
  it('没有轮次时返回空场景', () => {
    const scene = buildScene(story({ sections: [], latestSectionId: '' }));
    expect(scene.lanes).toHaveLength(0);
    expect(scene.final).toBeNull();
    expect(scene.summary.steps).toBe(0);
  });

  it('按技能切行，并把同一批设备叠成一张卡', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '检查设备' } }),
              step({ id: 's1', title: '读 3 号机', tool: 'read_device', metrics: [{ name: '出口压力', value: '0.62 MPa' }] }),
              step({ id: 's2', title: '读 1 号机', tool: 'read_device' }),
              step({ id: 's3', title: '读 2 号机', tool: 'read_device' }),
              step({ id: 's4', title: '判断', tone: 'think', detail: '负载上升' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes).toHaveLength(1);
    const cards = scene.lanes[0].cards;
    expect(cards[0].kind).toBe('skill');
    expect(cards[1].kind).toBe('device');
    expect(cards[1].items).toHaveLength(3);
    expect(cards[1].count).toBe(3);
    expect(cards[2].kind).toBe('conclusion');
    // 思考不占卡片，挂在第 2 张卡之前
    expect(scene.lanes[0].notes).toHaveLength(1);
    expect(scene.lanes[0].notes[0]).toMatchObject({ index: 2, text: '负载上升' });
    expect(scene.summary.steps).toBe(5);
  });

  it('同一技能内的不同工具不会叠在一起', () => {
    const cards: SceneCard[] = [
      { id: 'a', kind: 'device', at: '', title: '', sub: '', status: 'done', metrics: [] },
      { id: 'b', kind: 'doc', at: '', title: '', sub: '', status: 'done', metrics: [] },
      { id: 'c', kind: 'device', at: '', title: '', sub: '', status: 'done', metrics: [] },
    ];
    const grouped = groupCards(cards);
    expect(grouped.map((card) => card.kind)).toEqual(['device', 'doc', 'device']);
  });

  it('失败轮次标记失败并带错误卡', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            status: 'failed',
            error: '逆变器无响应',
            steps: [
              step({ id: 's0', title: '告警管理', tone: 'skill', skill: { name: 'alarm', title: '告警管理', description: '' } }),
              step({ id: 's1', title: '确认告警', tool: 'acknowledge_alarm', status: 'failed' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes[0].status).toBe('failed');
    expect(scene.lanes[0].cards.some((card) => card.kind === 'error')).toBe(true);
    expect(scene.final?.tone).toBe('warn');
    expect(scene.final?.status).toBe('部分完成');
  });

  it('提问与审批进入待确认，并带上计划中的下一步', () => {
    const scene = buildScene(story(), {
      question: {
        questionId: 'q1',
        questions: [
          {
            header: '是否停机',
            question: '3 号机现在停机复检滤芯？',
            options: [{ label: '现在停机', description: '' }, { label: '白天处理', description: '' }],
            multiSelect: false,
          },
        ],
      },
      approvals: [{ approvalId: 'a1', tool: 'set_device', summary: '调高变频器到 42 Hz' }],
      plans: [{ content: '白天复检滤芯', status: 'pending' }],
    });
    expect(scene.pending).toHaveLength(2);
    expect(scene.pending[0].title).toBe('是否停机');
    expect(scene.pending[0].options).toEqual(['现在停机', '白天处理']);
    expect(scene.final?.cols[2][1]).toBe('白天复检滤芯');
    expect(scene.final?.cols[2][2]).toBe('2 项待确认');
  });

  it('视觉证据生成画面卡，标题取设备名', () => {
    const scene = buildScene(
      story({
        vision: [
          { evidenceId: 'e1', deviceId: 'd1', deviceName: '东门通道', at: '17:37', caption: '现场画面', finding: '通道畅通', mime: 'image/jpeg' },
        ],
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({ id: 's1', title: '看画面', visionIds: ['e1'] }),
            ],
          }),
        ],
      }),
    );
    const vision = scene.lanes[0].cards.find((card) => card.kind === 'vision');
    expect(vision?.title).toBe('东门通道');
    expect(vision?.detail).toBe('通道畅通');
  });

  it('新轮次已有 skill/loaded 时不会重复切行', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({
                id: 's1',
                title: '结算技能',
                tool: 'complete_skill',
                tone: 'plan',
                rawResult: '下一个技能：视频巡检（video-inspection）。',
              }),
              step({ id: 's2', title: '视频巡检', tone: 'skill', skill: { name: 'video-inspection', title: '视频巡检', description: '' } }),
              step({ id: 's3', title: '看画面', visionIds: [] }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes).toHaveLength(2);
    expect(scene.lanes[1].skill).toBe('视频巡检');
  });

  it('失败的技能加载不切行，只作为当前行的失败卡片', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({ id: 's1', title: '查阅作业规范', tool: 'get_skill', tone: 'skill', status: 'failed' }),
              step({ id: 's2', title: '读设备', tool: 'read_device' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes).toHaveLength(1);
    expect(scene.lanes[0].cards.filter((card) => card.kind === 'skill')).toHaveLength(1);
    const failed = scene.lanes[0].cards.find((card) => card.status === 'failed');
    expect(failed?.title).toBe('查阅作业规范');
  });

  it('成功的 get_skill 工具步骤不会和 skill/loaded 重复成两张卡', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 'g0', title: '查阅「巡检」作业规范', tool: 'get_skill', status: 'done', rawResult: '# 巡检' }),
              step({
                id: 's0',
                title: '应用「巡检」作业规范',
                tone: 'skill',
                skill: { name: 'inspection', title: '巡检', description: '' },
              }),
              step({ id: 's1', title: '读设备', tool: 'read_device' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes).toHaveLength(1);
    expect(scene.lanes[0].skill).toBe('巡检');
    const skillCards = scene.lanes[0].cards.filter((card) => card.kind === 'skill');
    expect(skillCards).toHaveLength(1);
    expect(skillCards[0].title).toBe('巡检');
  });

  it('没有技能时的第一行叫「开场思考」并标记为 prologue', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [step({ id: 't0', title: 'AI 在思考', tone: 'think' }), step({ id: 'd0', title: '读设备', tool: 'read_device' })],
          }),
        ],
      }),
    );
    expect(scene.lanes[0].skill).toBe('开场思考');
    expect(scene.lanes[0].prologue).toBe(true);
    expect(scene.lanes[0].notes).toHaveLength(1);
  });

  it('判断依据带上它导致的下一个动作', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({ id: 't0', title: 'AI 在思考', tone: 'think', detail: '压力低，先提频。' }),
              step({ id: 'd0', title: '调高频率', tool: 'set_device', tone: 'write' }),
            ],
          }),
        ],
      }),
    );
    const note = scene.lanes[0].notes[0];
    expect(note.text).toBe('压力低，先提频。');
    expect(note.nextCardTitle).toBe('调高频率');
  });

  it('结算技能不占卡片，失败的 get_skill 留在当前行', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({
                id: 's1',
                title: '结算技能',
                tool: 'complete_skill',
                tone: 'plan',
                rawResult: '下一个技能：视频巡检（video-inspection）。',
              }),
              step({ id: 's2', title: '查阅作业规范', tool: 'get_skill', tone: 'skill', status: 'failed' }),
              step({ id: 's3', title: '看画面', visionIds: [] }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes.map((lane) => lane.skill)).toEqual(['巡检']);
    expect(scene.lanes[0].cards.some((card) => card.tool === 'complete_skill')).toBe(false);
    expect(scene.lanes[0].cards.some((card) => card.status === 'failed')).toBe(true);
  });

  it('未知 viewKey 自动落到通用卡，不需要改页面代码', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({ id: 's1', title: '导出报告', tool: 'export_report', viewKey: 'report.export' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes[0].cards.map((card) => card.kind)).toContain('generic');
  });

  it('同一个技能连续两次 skill/loaded 合并成一行', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 'a0', title: '视频巡检', tone: 'skill', skill: { name: 'video-inspection', title: '视频巡检', description: '' } }),
              step({ id: 'a1', title: '视频巡检', tone: 'skill', skill: { name: 'video-inspection', title: '视频巡检', description: '' } }),
              step({ id: 'a2', title: '看画面', visionIds: [] }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes).toHaveLength(1);
    expect(scene.lanes[0].skill).toBe('视频巡检');
    expect(scene.lanes[0].cards.filter((card) => card.kind === 'skill')).toHaveLength(1);
  });

  it('结算提示的下一个技能就是当前行时不再新开一行', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 'a0', title: '故障诊断', tone: 'skill', skill: { name: 'fault-diagnosis', title: '故障诊断', description: '' } }),
              step({
                id: 'a1',
                title: '结算技能',
                tool: 'complete_skill',
                tone: 'plan',
                rawResult: '下一个技能：故障诊断（fault-diagnosis）。',
              }),
              step({ id: 'a2', title: '读设备', tool: 'read_device' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.lanes).toHaveLength(1);
    expect(scene.lanes[0].cards.filter((card) => card.kind === 'skill')).toHaveLength(1);
  });

  it('相邻同名技能行会合并', () => {
    const card = (id: string, kind: SceneCard['kind'], title: string): SceneCard => ({
      id,
      kind,
      at: '',
      title,
      sub: '',
      status: 'done',
      metrics: [],
    });
    const lanes: SceneLane[] = [
      { id: 'l1', skillKey: 'fault-diagnosis', skill: '故障诊断', at: '', status: 'done', cards: [card('s1', 'skill', '故障诊断')], notes: [], steps: 1 },
      { id: 'l2', skillKey: 'fault-diagnosis', skill: '故障诊断', at: '', status: 'done', cards: [card('s2', 'skill', '故障诊断'), card('d2', 'device', '读设备')], notes: [], steps: 2 },
    ];
    const merged = mergeDuplicateLanes(lanes);
    expect(merged).toHaveLength(1);
    expect(merged[0].cards.map((item) => item.id)).toEqual(['s1', 'd2']);
    expect(merged[0].steps).toBe(2);
  });

  it('叠卡最多 5 张，超出拆成下一叠', () => {
    const devices: SceneCard[] = Array.from({ length: 14 }, (_, index) => ({
      id: `d${index}`,
      kind: 'device',
      at: '',
      title: `设备 ${index}`,
      sub: '',
      status: 'done',
      metrics: [],
    }));
    const grouped = groupCards(devices);
    expect(grouped).toHaveLength(3);
    expect(grouped.map((card) => card.items?.length)).toEqual([5, 5, 4]);
  });

  it('技能超过 12 个时给出未展示提示', () => {
    const steps = Array.from({ length: 14 }, (_, index) =>
      step({
        id: `skill-${index}`,
        title: `技能 ${index}`,
        tone: 'skill',
        skill: { name: `skill-${index}`, title: `技能 ${index}`, description: '' },
      }),
    );
    const scene = buildScene(story({ sections: [section({ steps })] }));
    expect(scene.lanes).toHaveLength(12);
    expect(scene.hiddenSkills).toBe(2);
  });

  it('失败的处置单独统计，不混进成功处置', () => {
    const scene = buildScene(
      story({
        sections: [
          section({
            steps: [
              step({ id: 's0', title: '巡检', tone: 'skill', skill: { name: 'inspection', title: '巡检', description: '' } }),
              step({ id: 'a1', title: '调整频率', tool: 'set_device', tone: 'write', status: 'done' }),
              step({ id: 'a2', title: '重启风机', tool: 'call_device', tone: 'write', status: 'failed' }),
            ],
          }),
        ],
      }),
    );
    expect(scene.summary.actions).toBe(1);
    expect(scene.summary.failedActions).toBe(1);
    expect(scene.final?.cols[0][2]).toContain('1 次处置');
    expect(scene.final?.cols[0][2]).toContain('1 次失败');
  });

  it('连续完全相同的工具调用合并成一张 ×N 卡片', () => {
    const steps = ['a', 'b', 'c', 'd'].map((id) =>
      step({ id, title: '检查当前告警', detail: '当前没有待处理告警', tool: 'alarm_list', tone: 'alarm' }),
    );
    const scene = buildScene(story({ sections: [section({ steps })] }));
    const findings = scene.lanes[0].cards.filter((card) => card.kind === 'finding');
    expect(findings).toHaveLength(1);
    expect(findings[0].repeatCount).toBe(4);
  });

  it('内容不同的连续调用不会被合并', () => {
    const steps = [
      step({ id: 'a', title: '读取 A', detail: '在线', tool: 'read_device' }),
      step({ id: 'b', title: '读取 B', detail: '在线', tool: 'read_device' }),
    ];
    const scene = buildScene(story({ sections: [section({ steps })] }));
    const devices = scene.lanes[0].cards.filter((card) => card.kind === 'device');
    expect(devices).toHaveLength(1);
    expect(devices[0].items).toHaveLength(2);
    expect(devices[0].repeatCount).toBeUndefined();
  });

  it('结果相同但参数不同的调用不会被合并', () => {
    const steps = [
      step({ id: 'a', title: '检查当前告警', detail: '当前没有待处理告警', tool: 'alarm_list', tone: 'alarm', argsKey: '{"levels":["critical"]}' }),
      step({ id: 'b', title: '检查当前告警', detail: '当前没有待处理告警', tool: 'alarm_list', tone: 'alarm', argsKey: '{"levels":["warning"]}' }),
    ];
    const scene = buildScene(story({ sections: [section({ steps })] }));
    const findings = scene.lanes[0].cards.filter((card) => card.kind === 'finding');
    expect(findings).toHaveLength(2);
    expect(findings[0].repeatCount).toBeUndefined();
  });

  it('参数相同的调用仍然合并，并保留参数用于卡片展示', () => {
    const params = [{ label: '级别', value: 'critical' }];
    const steps = ['a', 'b'].map((id) =>
      step({ id, title: '检查当前告警', detail: '当前没有待处理告警', tool: 'alarm_list', tone: 'alarm', argsKey: '{"levels":["critical"]}', params }),
    );
    const scene = buildScene(story({ sections: [section({ steps })] }));
    const findings = scene.lanes[0].cards.filter((card) => card.kind === 'finding');
    expect(findings).toHaveLength(1);
    expect(findings[0].repeatCount).toBe(2);
    expect(findings[0].params).toEqual(params);
  });
});
