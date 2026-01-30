'use client'

import React from 'react'
import { 
  RiDeviceLine, 
  RiWifiLine, 
  RiAlarmWarningLine, 
  RiHeartPulseLine,
  RiArrowUpLine,
  RiArrowDownLine
} from '@remixicon/react'
import type { DashboardStats } from '@/types'
import cn from '@/utils/classnames'

interface StatsCardsProps {
  stats: DashboardStats | null
  loading?: boolean
}

const StatsCards = ({ stats, loading }: StatsCardsProps) => {
  const getSystemStatusColor = (status?: string) => {
    switch (status) {
      case 'healthy':
        return 'bg-text-success'
      case 'warning':
        return 'bg-text-warning'
      case 'error':
        return 'bg-text-destructive'
      default:
        return 'bg-text-tertiary'
    }
  }

  const getSystemStatusText = (status?: string) => {
    switch (status) {
      case 'healthy':
        return '正常'
      case 'warning':
        return '警告'
      case 'error':
        return '故障'
      default:
        return '未知'
    }
  }

  const formatUptime = (seconds?: number) => {
    if (!seconds) return '--'
    const days = Math.floor(seconds / 86400)
    const hours = Math.floor((seconds % 86400) / 3600)
    return `${days}天${hours}小时`
  }

  const formatNumber = (num?: number) => {
    if (num === undefined || num === null) return '--'
    return num.toLocaleString()
  }

  const formatPercentage = (current?: number, total?: number) => {
    if (!current || !total) return '--'
    return `${((current / total) * 100).toFixed(1)}%`
  }

  if (loading) {
    return (
      <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4">
        {[...Array(4)].map((_, i) => (
          <div key={i} className="bg-components-panel-bg overflow-hidden shadow rounded-lg border border-divider-subtle animate-pulse">
            <div className="p-5">
              <div className="flex items-center">
                <div className="flex-shrink-0">
                  <div className="w-8 h-8 bg-components-panel-bg-alt rounded-md"></div>
                </div>
                <div className="ml-5 w-0 flex-1">
                  <div className="h-4 bg-components-panel-bg-alt rounded mb-2"></div>
                  <div className="h-6 bg-components-panel-bg-alt rounded"></div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4">
      {/* 设备总数 */}
      <div className="bg-components-panel-bg overflow-hidden shadow rounded-lg border border-divider-subtle">
        <div className="p-5">
          <div className="flex items-center">
            <div className="flex-shrink-0">
              <div className="w-8 h-8 bg-components-button-primary-bg rounded-md flex items-center justify-center">
                <RiDeviceLine className="w-4 h-4 text-white" />
              </div>
            </div>
            <div className="ml-5 w-0 flex-1">
              <dl>
                <dt className="text-sm font-medium text-text-tertiary truncate">
                  设备总数
                </dt>
                <dd className="flex items-baseline">
                  <div className="text-2xl font-semibold text-text-primary">
                    {formatNumber(stats?.totalDevices)}
                  </div>
                  {stats?.monthlyGrowth?.devices !== undefined && (
                    <div className={cn(
                      "ml-2 flex items-baseline text-sm font-semibold",
                      stats.monthlyGrowth.devices >= 0 ? "text-text-success" : "text-text-destructive"
                    )}>
                      {stats.monthlyGrowth.devices >= 0 ? (
                        <RiArrowUpLine className="w-3 h-3 mr-0.5" />
                      ) : (
                        <RiArrowDownLine className="w-3 h-3 mr-0.5" />
                      )}
                      {Math.abs(stats.monthlyGrowth.devices)}
                    </div>
                  )}
                </dd>
                <dd className="text-xs text-text-tertiary">
                  本月新增设备
                </dd>
              </dl>
            </div>
          </div>
        </div>
      </div>

      {/* 在线设备 */}
      <div className="bg-components-panel-bg overflow-hidden shadow rounded-lg border border-divider-subtle">
        <div className="p-5">
          <div className="flex items-center">
            <div className="flex-shrink-0">
              <div className="w-8 h-8 bg-text-success rounded-md flex items-center justify-center">
                <RiWifiLine className="w-4 h-4 text-white" />
              </div>
            </div>
            <div className="ml-5 w-0 flex-1">
              <dl>
                <dt className="text-sm font-medium text-text-tertiary truncate">
                  在线设备
                </dt>
                <dd className="flex items-baseline">
                  <div className="text-2xl font-semibold text-text-primary">
                    {formatNumber(stats?.onlineDevices)}
                  </div>
                  <div className="ml-2 text-sm font-medium text-text-success">
                    {formatPercentage(stats?.onlineDevices, stats?.totalDevices)}
                  </div>
                </dd>
                <dd className="text-xs text-text-tertiary">
                  设备在线率
                </dd>
              </dl>
            </div>
          </div>
        </div>
      </div>

      {/* 活跃告警 */}
      <div className="bg-components-panel-bg overflow-hidden shadow rounded-lg border border-divider-subtle">
        <div className="p-5">
          <div className="flex items-center">
            <div className="flex-shrink-0">
              <div className="w-8 h-8 bg-text-warning rounded-md flex items-center justify-center">
                <RiAlarmWarningLine className="w-4 h-4 text-white" />
              </div>
            </div>
            <div className="ml-5 w-0 flex-1">
              <dl>
                <dt className="text-sm font-medium text-text-tertiary truncate">
                  活跃告警
                </dt>
                <dd className="flex items-baseline">
                  <div className="text-2xl font-semibold text-text-primary">
                    {formatNumber(stats?.activeAlarms)}
                  </div>
                </dd>
                <dd className="text-xs text-text-tertiary">
                  需要处理的告警
                </dd>
              </dl>
            </div>
          </div>
        </div>
      </div>

      {/* 系统状态 */}
      <div className="bg-components-panel-bg overflow-hidden shadow rounded-lg border border-divider-subtle">
        <div className="p-5">
          <div className="flex items-center">
            <div className="flex-shrink-0">
              <div className={cn(
                "w-8 h-8 rounded-md flex items-center justify-center",
                getSystemStatusColor(stats?.systemStatus)
              )}>
                <RiHeartPulseLine className="w-4 h-4 text-white" />
              </div>
            </div>
            <div className="ml-5 w-0 flex-1">
              <dl>
                <dt className="text-sm font-medium text-text-tertiary truncate">
                  系统状态
                </dt>
                <dd className="flex items-baseline">
                  <div className="text-2xl font-semibold text-text-primary">
                    {getSystemStatusText(stats?.systemStatus)}
                  </div>
                </dd>
                <dd className="text-xs text-text-tertiary">
                  运行时间: {formatUptime(stats?.systemUptime)}
                </dd>
              </dl>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default StatsCards