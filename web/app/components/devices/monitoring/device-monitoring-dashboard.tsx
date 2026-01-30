'use client'

import React, { useState } from 'react'
import { 
  RiRefreshLine,
  RiSettings3Line,
  RiAlarmWarningLine,
  RiBarChartLine,
  RiFileTextLine,
  RiTimeLine
} from '@remixicon/react'
import { useDeviceStatus, useDeviceMetrics, useDevicePerformance, useDevicePerformanceAlerts, useDeviceTraceStatistics } from '@/service/device-monitoring'
import DeviceStatusCard from './device-status-card'
import DevicePerformanceMetrics from './device-performance-metrics'
import DevicePerformanceChart from './device-performance-chart'
import DevicePerformanceAlerts from './device-performance-alerts'
import DeviceTraceOverview from './device-trace-overview'
import DeviceTraceRecords from './device-trace-records'
import cn from '@/utils/classnames'

interface DeviceMonitoringDashboardProps {
  deviceId: string
  deviceName?: string
}

type TabType = 'overview' | 'performance' | 'alerts' | 'traces'

const DeviceMonitoringDashboard = ({ deviceId, deviceName }: DeviceMonitoringDashboardProps) => {
  const [activeTab, setActiveTab] = useState<TabType>('overview')
  const [refreshKey, setRefreshKey] = useState(0)

  // 获取设备监控数据
  const { data: deviceStatus, isLoading: statusLoading, refetch: refetchStatus } = useDeviceStatus(deviceId)
  const { data: deviceMetrics, isLoading: metricsLoading, refetch: refetchMetrics } = useDeviceMetrics(deviceId)
  const { data: performanceMetrics, isLoading: performanceLoading, refetch: refetchPerformance } = useDevicePerformance(deviceId)
  const { data: performanceAlerts, isLoading: alertsLoading, refetch: refetchAlerts } = useDevicePerformanceAlerts(deviceId)
  const { data: traceStats, isLoading: traceStatsLoading, refetch: refetchTraceStats } = useDeviceTraceStatistics(deviceId)

  const handleRefresh = async () => {
    setRefreshKey(prev => prev + 1)
    await Promise.all([
      refetchStatus(),
      refetchMetrics(),
      refetchPerformance(),
      refetchAlerts(),
      refetchTraceStats()
    ])
  }

  const tabs = [
    {
      id: 'overview' as TabType,
      name: '概览',
      icon: RiBarChartLine,
      description: '设备状态和基本信息'
    },
    {
      id: 'performance' as TabType,
      name: '性能监控',
      icon: RiTimeLine,
      description: '性能指标和历史趋势'
    },
    {
      id: 'alerts' as TabType,
      name: '性能告警',
      icon: RiAlarmWarningLine,
      description: '性能告警和异常',
      badge: performanceAlerts?.length || 0
    },
    {
      id: 'traces' as TabType,
      name: '追踪记录',
      icon: RiFileTextLine,
      description: '操作日志和追踪信息'
    }
  ]

  return (
    <div className="space-y-6">
      {/* 头部 */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-text-primary">设备监控</h2>
          {deviceName && (
            <p className="text-sm text-text-tertiary mt-1">
              设备: {deviceName} ({deviceId})
            </p>
          )}
        </div>
        <div className="flex items-center space-x-3">
          <button
            onClick={handleRefresh}
            className="inline-flex items-center px-3 py-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium text-text-secondary bg-components-panel-bg hover:bg-components-panel-bg-alt focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-components-button-primary-bg"
          >
            <RiRefreshLine className="w-4 h-4 mr-2" />
            刷新
          </button>
          <button className="inline-flex items-center px-3 py-2 border border-divider-subtle rounded-md shadow-sm text-sm font-medium text-text-secondary bg-components-panel-bg hover:bg-components-panel-bg-alt focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-components-button-primary-bg">
            <RiSettings3Line className="w-4 h-4 mr-2" />
            设置
          </button>
        </div>
      </div>

      {/* 标签页导航 */}
      <div className="border-b border-divider-subtle">
        <nav className="-mb-px flex space-x-8">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={cn(
                "group inline-flex items-center py-4 px-1 border-b-2 font-medium text-sm relative",
                activeTab === tab.id
                  ? "border-components-button-primary-bg text-components-button-primary-bg"
                  : "border-transparent text-text-tertiary hover:text-text-secondary hover:border-divider-subtle"
              )}
            >
              <tab.icon className="w-5 h-5 mr-2" />
              {tab.name}
              {tab.badge && tab.badge > 0 && (
                <span className="ml-2 inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-text-destructive text-white">
                  {tab.badge}
                </span>
              )}
            </button>
          ))}
        </nav>
      </div>

      {/* 标签页内容 */}
      <div className="space-y-6">
        {activeTab === 'overview' && (
          <div className="space-y-6">
            {/* 设备状态卡片 */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              <DeviceStatusCard 
                deviceStatus={deviceStatus}
                deviceMetrics={deviceMetrics}
                loading={statusLoading || metricsLoading}
              />
              <DevicePerformanceMetrics
                performanceMetrics={performanceMetrics}
                loading={performanceLoading}
                compact
              />
            </div>

            {/* 追踪记录概览 */}
            <DeviceTraceOverview
              traceStats={traceStats}
              loading={traceStatsLoading}
            />
          </div>
        )}

        {activeTab === 'performance' && (
          <div className="space-y-6">
            {/* 性能指标 */}
            <DevicePerformanceMetrics
              performanceMetrics={performanceMetrics}
              loading={performanceLoading}
            />

            {/* 性能趋势图表 */}
            <DevicePerformanceChart
              deviceId={deviceId}
              refreshKey={refreshKey}
            />
          </div>
        )}

        {activeTab === 'alerts' && (
          <DevicePerformanceAlerts
            alerts={performanceAlerts}
            loading={alertsLoading}
          />
        )}

        {activeTab === 'traces' && (
          <DeviceTraceRecords
            deviceId={deviceId}
            refreshKey={refreshKey}
          />
        )}
      </div>
    </div>
  )
}

export default DeviceMonitoringDashboard