'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import AlarmList from '@/app/components/alarm/alarm-list'
import AlarmStatistics from '@/app/components/alarm/alarm-statistics'
import AlarmRuleList from '@/app/components/alarm/alarm-rule-list'
import TabSliderNew from '@/app/components/base/tab-slider-new'

export default function AlarmsPage() {
  const { t } = useTranslation('common')
  const [activeTab, setActiveTab] = useState('alarms')

  const tabs = [
    { value: 'alarms', text: '报警列表' },
    { value: 'rules', text: '报警规则' },
  ]

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary">报警管理</h1>
          <p className="mt-1 text-sm text-text-secondary">
            查看和管理设备报警信息
          </p>
        </div>
      </div>

      {/* 统计卡片 */}
      <AlarmStatistics />

      {/* 标签页 */}
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-4 border-b border-divider-subtle">
          <TabSliderNew
            value={activeTab}
            onChange={setActiveTab}
            options={tabs}
          />
        </div>

        <div className="px-4 py-5 sm:p-6">
          {activeTab === 'alarms' && <AlarmList />}
          {activeTab === 'rules' && <AlarmRuleList />}
        </div>
      </div>
    </div>
  )
}