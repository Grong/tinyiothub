'use client'

import { useTranslation } from 'react-i18next'
import {
  RiAlarmWarningLine,
  RiCheckLine,
  RiCloseLine,
  RiErrorWarningLine,
} from '@remixicon/react'
import { useAlarmStatistics } from '@/service/alarms'
import Loading from '@/app/components/base/loading'

interface AlarmStatisticsProps {
  startTime?: string
  endTime?: string
}

const AlarmStatistics: React.FC<AlarmStatisticsProps> = ({ startTime, endTime }) => {
  const { t } = useTranslation()
  const { data, isLoading } = useAlarmStatistics({ startTime, endTime })

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-32">
        <Loading />
      </div>
    )
  }

  const stats = [
    {
      label: '总报警数',
      value: data?.totalCount || 0,
      icon: <RiAlarmWarningLine className="w-6 h-6" />,
      color: 'text-text-accent bg-background-accent-subtle',
    },
    {
      label: '活跃报警',
      value: data?.activeCount || 0,
      icon: <RiErrorWarningLine className="w-6 h-6" />,
      color: 'text-text-destructive bg-background-destructive-subtle',
    },
    {
      label: '已确认',
      value: data?.acknowledgedCount || 0,
      icon: <RiCheckLine className="w-6 h-6" />,
      color: 'text-text-warning bg-background-warning-subtle',
    },
    {
      label: '已解决',
      value: data?.resolvedCount || 0,
      icon: <RiCloseLine className="w-6 h-6" />,
      color: 'text-text-success bg-background-success-subtle',
    },
  ]

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      {stats.map((stat, index) => (
        <div
          key={index}
          className="border border-divider-subtle rounded-lg p-4 bg-components-panel-bg hover:shadow-md transition-shadow"
        >
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm text-text-tertiary mb-1">{stat.label}</div>
              <div className="text-2xl font-bold text-text-primary">{stat.value}</div>
            </div>
            <div className={`p-3 rounded-lg ${stat.color}`}>
              {stat.icon}
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}

export default AlarmStatistics
