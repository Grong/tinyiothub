/** User-facing skill names/descriptions for event rows and story steps. */
const SKILL_COPY: Record<string, { title: string; description: string }> = {
  inspection: { title: '设备巡检', description: '检查离线设备和高优先级告警，生成设备状态摘要。' },
  'alarm-management': { title: '告警管理', description: '查询设备告警、确认告警并按严重程度给出处置建议。' },
  'device-management': { title: '设备管理', description: '查看设备清单和状态，读写设备属性并执行设备动作。' },
  'driver-management': { title: '驱动管理', description: '查看和测试 IoT 设备接入驱动。' },
  'heartbeat-monitor': { title: '定时巡检', description: '按计划巡检设备健康、告警和数据连续性，主动报告异常。' },
  'job-management': { title: '定时任务管理', description: '管理定时任务和自动化规则，让设备操作按计划执行。' },
  'platform-overview': { title: '平台总览', description: 'TinyIoTHub 的平台能力、Agent 角色和工具使用总览。' },
  troubleshooting: { title: '故障诊断', description: '诊断设备异常，结合事件、告警和知识库定位原因并给出修复建议。' },
  'energy-saving': { title: '节能分析', description: '基于真实能耗和运行数据给出节能建议。' },
  visualization: { title: '可视化展示', description: '用卡片、表格、图表或控制面板展示设备和告警。' },
  workspace: { title: '工作空间', description: '查找场景并展示空间与设备信息。' },
};

export function skillDisplay(name: string, title?: string, description?: string): { title: string; description: string } {
  const known = SKILL_COPY[name];
  return {
    title: title || known?.title || name || '作业规范',
    description: description || known?.description || '',
  };
}
