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
    <div className="flex h-full w-full flex-col overflow-hidden bg-background-body">
      <div className="flex flex-wrap items-center justify-between gap-y-2 bg-background-body px-12 pb-2 pt-4 leading-[56px]">
        <TabSliderNew
          value={activeTab}
          onChange={setActiveTab}
          options={tabs}
        />
      </div>
      <div className="flex-1 overflow-y-auto px-12 pb-4 space-y-4">
        {/* 统计卡片 */}
        <AlarmStatistics />

        {/* 内容区域 */}
        {activeTab === 'alarms' && <AlarmList />}
        {activeTab === 'rules' && <AlarmRuleList />}
      </div>
    </div>
  )
}