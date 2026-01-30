'use client'

import React from 'react'

type ChartViewProps = {
  deviceId: string
}

const ChartView = ({ deviceId }: ChartViewProps) => {
  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <div className="bg-components-panel-bg rounded-xl p-6 shadow-sm border border-divider-subtle">
        <h3 className="text-lg font-semibold text-text-primary mb-4">数据图表</h3>
        <div className="flex items-center justify-center h-64 text-text-tertiary">
          <div className="text-center">
            <div className="text-sm">图表功能即将推出</div>
            <div className="text-xs mt-1">在这里您将能够查看设备数据的可视化图表</div>
          </div>
        </div>
      </div>

      <div className="bg-components-panel-bg rounded-xl p-6 shadow-sm border border-divider-subtle">
        <h3 className="text-lg font-semibold text-text-primary mb-4">最近事件</h3>
        <div className="flex items-center justify-center h-64 text-text-tertiary">
          <div className="text-center">
            <div className="text-sm">事件功能即将推出</div>
            <div className="text-xs mt-1">在这里您将能够查看设备的历史事件和日志</div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default ChartView