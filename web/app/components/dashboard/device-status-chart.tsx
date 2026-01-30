'use client'

import React from 'react'
import type { DeviceStatusDistribution } from '@/types'

interface DeviceStatusChartProps {
  data: DeviceStatusDistribution | null
  loading?: boolean
}

const DeviceStatusChart = ({ data, loading }: DeviceStatusChartProps) => {
  if (loading) {
    return (
      <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
        <div className="px-4 py-5 sm:p-6">
          <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
            设备状态分布
          </h3>
          <div className="animate-pulse">
            <div className="flex justify-center mb-6">
              <div className="w-48 h-48 bg-components-panel-bg-alt rounded-full"></div>
            </div>
            <div className="space-y-3">
              {[...Array(4)].map((_, i) => (
                <div key={i} className="flex items-center justify-between">
                  <div className="flex items-center">
                    <div className="w-3 h-3 bg-components-panel-bg-alt rounded-full mr-2"></div>
                    <div className="h-4 bg-components-panel-bg-alt rounded w-16"></div>
                  </div>
                  <div className="h-4 bg-components-panel-bg-alt rounded w-8"></div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    )
  }

  const total = data ? data.online + data.offline + data.error + data.maintenance : 0
  
  const statusData = [
    { 
      name: '在线', 
      value: data?.online || 0, 
      color: 'bg-text-success',
      percentage: total > 0 ? ((data?.online || 0) / total * 100).toFixed(1) : '0'
    },
    { 
      name: '离线', 
      value: data?.offline || 0, 
      color: 'bg-text-tertiary',
      percentage: total > 0 ? ((data?.offline || 0) / total * 100).toFixed(1) : '0'
    },
    { 
      name: '故障', 
      value: data?.error || 0, 
      color: 'bg-text-destructive',
      percentage: total > 0 ? ((data?.error || 0) / total * 100).toFixed(1) : '0'
    },
    { 
      name: '维护', 
      value: data?.maintenance || 0, 
      color: 'bg-text-warning',
      percentage: total > 0 ? ((data?.maintenance || 0) / total * 100).toFixed(1) : '0'
    },
  ]

  return (
    <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
      <div className="px-4 py-5 sm:p-6">
        <h3 className="text-lg leading-6 font-medium text-text-primary mb-4">
          设备状态分布
        </h3>
        
        {total === 0 ? (
          <div className="text-center py-12 text-text-tertiary">
            <div className="text-sm">暂无设备数据</div>
          </div>
        ) : (
          <div>
            {/* 简化的饼图显示 */}
            <div className="flex justify-center mb-6">
              <div className="relative w-48 h-48">
                <div className="absolute inset-0 flex items-center justify-center">
                  <div className="text-center">
                    <div className="text-2xl font-bold text-text-primary">{total}</div>
                    <div className="text-sm text-text-tertiary">设备总数</div>
                  </div>
                </div>
                {/* 这里可以后续集成真正的图表库 */}
                <div className="w-full h-full border-8 border-components-panel-bg-alt rounded-full"></div>
              </div>
            </div>

            {/* 状态列表 */}
            <div className="space-y-3">
              {statusData.map((item, index) => (
                <div key={index} className="flex items-center justify-between">
                  <div className="flex items-center">
                    <div className={`w-3 h-3 rounded-full mr-3 ${item.color}`}></div>
                    <span className="text-sm font-medium text-text-primary">{item.name}</span>
                  </div>
                  <div className="flex items-center space-x-2">
                    <span className="text-sm font-medium text-text-primary">{item.value}</span>
                    <span className="text-xs text-text-tertiary">({item.percentage}%)</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

export default DeviceStatusChart