'use client'

import React from 'react'
import StatsCards from '@/app/components/dashboard/stats-cards'
import DeviceStatusChart from '@/app/components/dashboard/device-status-chart'
import RecentAlarms from '@/app/components/dashboard/recent-alarms'
import SystemMetrics from '@/app/components/dashboard/system-metrics'
import QuickDevices from '@/app/components/dashboard/quick-devices'
import {
  useDashboardStats,
  useDeviceDistribution,
  useRecentAlarms,
  useSystemMetrics,
  useQuickDevices
} from '@/service/dashboard'

export default function DashboardPage() {
  const { data: stats, isLoading: statsLoading } = useDashboardStats()
  const { data: deviceDistribution, isLoading: distributionLoading } = useDeviceDistribution()
  const { data: recentAlarms, isLoading: alarmsLoading } = useRecentAlarms(8)
  const { data: systemMetrics, isLoading: metricsLoading } = useSystemMetrics()
  const { data: quickDevices, isLoading: devicesLoading } = useQuickDevices(8)

  return (
    <div className="space-y-6 p-6">
      {/* 页面标题 */}
      <div>
        <h1 className="text-2xl font-semibold text-text-primary">仪表板</h1>
        <p className="mt-1 text-sm text-text-secondary">
          实时监控系统状态和设备运行情况
        </p>
      </div>

      {/* 统计卡片 */}
      <StatsCards stats={stats?.result || null} loading={statsLoading} />

      {/* 主要内容区域 */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* 左侧列 */}
        <div className="lg:col-span-2 space-y-6">
          {/* 设备状态分布 */}
          <DeviceStatusChart 
            data={deviceDistribution?.result || null} 
            loading={distributionLoading} 
          />
          
          {/* 系统性能指标 */}
          <SystemMetrics 
            metrics={systemMetrics?.result || null} 
            loading={metricsLoading} 
          />
        </div>

        {/* 右侧列 */}
        <div className="space-y-6">
          {/* 最新告警 */}
          <RecentAlarms 
            alarms={recentAlarms?.result || null} 
            loading={alarmsLoading} 
          />
          
          {/* 关键设备 */}
          <QuickDevices 
            devices={quickDevices?.result || null} 
            loading={devicesLoading} 
          />
        </div>
      </div>
    </div>
  )
}