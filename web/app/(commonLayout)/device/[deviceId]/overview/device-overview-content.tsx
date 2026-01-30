'use client'
import React, { useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { Card, CardContent, CardHeader, CardTitle } from '@/app/components/base/card'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'
import Label from '@/app/components/base/label'
import Modal from '@/app/components/base/modal'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/app/components/base/tabs'
import { DeviceEvents } from '@/app/components/device/device-events'
import { useToast } from '@/hooks/use-toast'
import dayjs from 'dayjs'
import { 
  RiPulseLine, 
  RiTimeLine, 
  RiRefreshLine,
  RiTerminalBoxLine,
  RiPlayLine,
  RiSettings3Line,
  RiCheckboxCircleLine,
  RiCloseCircleLine,
  RiAlarmWarningLine,
  RiInformationLine,
  RiSearchLine,
  RiSignalWifiLine,
  RiSignalWifiOffLine,
  RiLineChartLine,
  RiBarChartBoxLine
} from '@remixicon/react'
import { deviceService, useDeviceProfile } from '@/service/devices'
import type { DeviceProperty, DeviceCommand } from '@/types/device'

interface DeviceOverviewContentProps {
  deviceId: string
}

// 生成模拟的当天曲线数据
const generateMiniChartData = (property: DeviceProperty) => {
  const points = 24 // 24小时的数据点
  const data = []
  const baseValue = parseFloat(property.value?.toString() || '50')
  
  for (let i = 0; i < points; i++) {
    const variation = (Math.random() - 0.5) * 20 // ±10的变化
    const value = Math.max(0, baseValue + variation)
    data.push(value)
  }
  
  return data
}

// 判断属性是否为数值类型（可显示曲线）
const isNumericProperty = (property: DeviceProperty): boolean => {
  // 检查数据类型
  if (property.dataType === 'number' || property.dataType === 'float' || property.dataType === 'integer') {
    return true
  }
  
  // 检查数据类型为string但值是数字的情况
  if (property.dataType === 'string') {
    return false
  }
  
  // 排除布尔类型
  if (property.dataType === 'boolean') {
    return false
  }
  
  // 对于未知类型，尝试解析值
  const value = property.currentValue !== undefined ? property.currentValue : property.value
  if (value === null || value === undefined) {
    return false
  }
  
  return !isNaN(parseFloat(value.toString())) && isFinite(parseFloat(value.toString()))
}

// 增强的迷你曲线组件 - 适配主题颜色
const MiniChart: React.FC<{ 
  data: number[]
  color?: string
  className?: string
}> = ({ 
  data, 
  color, 
  className = '' 
}) => {
  if (!data || data.length === 0) return null
  
  // 使用主题颜色，如果没有指定颜色
  const chartColor = color || 'var(--color-components-chart-line)'
  
  const max = Math.max(...data)
  const min = Math.min(...data)
  const range = max - min || 1
  
  const points = data.map((value, index) => {
    const x = (index / (data.length - 1)) * 60 // 60px width
    const y = 20 - ((value - min) / range) * 20 // 20px height, inverted
    return `${x},${y}`
  }).join(' ')
  
  return (
    <svg width="60" height="20" className={`flex-shrink-0 ${className}`}>
      <polyline
        points={points}
        fill="none"
        stroke={chartColor}
        strokeWidth="1.5"
        className="opacity-80"
      />
    </svg>
  )
}

const DeviceOverviewContent: React.FC<DeviceOverviewContentProps> = ({ deviceId }) => {
  const [selectedCommand, setSelectedCommand] = useState<DeviceCommand | null>(null)
  const [commandParams, setCommandParams] = useState<Record<string, any>>({})
  const [isExecuteDialogOpen, setIsExecuteDialogOpen] = useState(false)
  const [selectedProperty, setSelectedProperty] = useState<DeviceProperty | null>(null)
  const [isChartDialogOpen, setIsChartDialogOpen] = useState(false)
  const { toast } = useToast()

  // 获取设备Profile（完整信息）- 添加自动刷新
  const { data: profile, isLoading, refetch } = useDeviceProfile(deviceId, {
    refetchInterval: 3000, // 每3秒自动刷新
    refetchIntervalInBackground: true, // 后台也刷新
  })

  // 调试信息：打印接收到的Profile数据
  React.useEffect(() => {
    if (profile && profile.properties && profile.properties.length > 0) {
      console.log('Profile received:', profile)
      console.log('First property raw data:', profile.properties[0])
      console.log('Properties with current_value:', profile.properties.filter(p => (p as any).current_value !== undefined))
    }
  }, [profile])

  // 移除手动定时器，因为已经使用了 React Query 的自动刷新

  // 执行指令
  const executeCommandMutation = useMutation({
    mutationFn: (params: { commandId: string; parameters: Record<string, any> }) =>
      deviceService.executeCommand(deviceId, params.commandId, params.parameters),
    onSuccess: () => {
      toast.success('指令执行成功')
      setIsExecuteDialogOpen(false)
      setSelectedCommand(null)
      setCommandParams({})
      // 刷新Profile数据
      refetch()
    },
    onError: (error: any) => {
      toast.error(error.message || '执行设备指令时发生错误')
    },
  })

  const handleExecuteCommand = () => {
    if (!selectedCommand) return
    
    executeCommandMutation.mutate({
      commandId: selectedCommand.id,
      parameters: commandParams,
    })
  }

  const openExecuteDialog = (command: DeviceCommand) => {
    setSelectedCommand(command)
    setCommandParams({})
    setIsExecuteDialogOpen(true)
  }

  const openChartDialog = (property: DeviceProperty) => {
    setSelectedProperty(property)
    setIsChartDialogOpen(true)
  }

  // 获取属性状态颜色（用于圆形状态指示器）- 使用主题颜色
  const getPropertyStatusColor = (property: DeviceProperty) => {
    // 使用转换后的字段
    if (property.alarmStatus === 2) return 'bg-state-destructive-solid' // 红色 - 高报警
    if (property.alarmStatus === 1) return 'bg-state-warning-solid' // 橙色 - 低报警
    
    // 检查值是否有效
    const value = property.currentValue !== undefined ? property.currentValue : property.value
    if (value === null || value === undefined) {
      return 'bg-text-tertiary' // 浅灰色 - 未知
    }
    
    return 'bg-state-success-solid' // 绿色 - 正常
  }

  // 获取属性状态文本
  const getPropertyStatusText = (property: DeviceProperty) => {
    if (property.alarmStatus === 2) return '高报警'
    if (property.alarmStatus === 1) return '低报警'
    
    const updateTime = property.lastUpdateTime || property.updatedAt
    if (updateTime) {
      const lastUpdate = new Date(updateTime)
      const now = new Date()
      const diffMinutes = (now.getTime() - lastUpdate.getTime()) / (1000 * 60)
      
      if (diffMinutes > 30) return '离线'
    }
    
    const value = property.currentValue !== undefined ? property.currentValue : property.value
    if (value === null || value === undefined) {
      return '未知'
    }
    
    return '正常'
  }
  const formatPropertyValue = (property: DeviceProperty) => {
    // API客户端已经自动转换字段，直接使用 camelCase 字段
    const value = property.currentValue !== undefined 
      ? property.currentValue 
      : property.value
    
    if (value === null || value === undefined) return '--'
    
    switch (property.dataType) {
      case 'boolean':
        return value ? '开启' : '关闭'
      case 'number':
        return String(value)
      default:
        return String(value)
    }
  }

  // 获取属性单位
  const getPropertyUnit = (property: DeviceProperty) => {
    return property.unit || ''
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <RiRefreshLine className="w-6 h-6 animate-spin text-text-tertiary" />
        <span className="ml-2 text-text-secondary">加载设备信息中...</span>
      </div>
    )
  }

  if (!profile) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center">
          <RiCloseCircleLine className="w-12 h-12 text-text-quaternary mx-auto mb-4" />
          <h3 className="text-lg font-medium text-text-primary mb-2">设备信息加载失败</h3>
          <p className="text-text-tertiary mb-4">无法获取设备详细信息</p>
          <Button onClick={() => refetch()}>重试</Button>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 精简的设备状态栏 */}
      <Card>
        <CardContent className="p-6">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-6">
              <div className="flex items-center space-x-3">
                {profile.isOnline ? (
                  <RiSignalWifiLine className="w-6 h-6 text-text-success" />
                ) : (
                  <RiSignalWifiOffLine className="w-6 h-6 text-text-quaternary" />
                )}
                <div>
                  <h1 className="text-xl font-semibold text-text-primary">{profile.device.name}</h1>
                  <p className="text-sm text-text-tertiary">
                    {profile.isOnline ? '在线' : '离线'} • {profile.device.deviceType || '未知类型'}
                  </p>
                </div>
              </div>
              
              {/* 关键统计数据 */}
              <div className="flex items-center space-x-8">
                <div className="text-center">
                  <div className="text-2xl font-bold text-text-accent">{profile.overview.totalProperties}</div>
                  <div className="text-xs text-text-quaternary">属性</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold text-text-success">{profile.overview.totalCommands}</div>
                  <div className="text-xs text-text-quaternary">指令</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold text-text-warning">{profile.overview.totalEvents}</div>
                  <div className="text-xs text-text-quaternary">事件</div>
                </div>
                <div className="text-center">
                  <div className="text-2xl font-bold text-text-destructive">{profile.overview.activeAlarms}</div>
                  <div className="text-xs text-text-quaternary">告警</div>
                </div>
              </div>
            </div>
            
            <Button onClick={() => refetch()} variant="secondary" size="small">
              <RiRefreshLine className="w-4 h-4 mr-2" />
              刷新
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* 主要内容标签页 - 提升到顶部 */}
      <Tabs defaultValue="properties" className="w-full">
        <TabsList className="mb-6">
          <TabsTrigger value="properties" className="flex items-center space-x-2">
            <RiPulseLine className="w-4 h-4" />
            <span>属性 ({profile.properties?.length || 0})</span>
          </TabsTrigger>
          <TabsTrigger value="commands" className="flex items-center space-x-2">
            <RiTerminalBoxLine className="w-4 h-4" />
            <span>指令 ({profile.commands?.length || 0})</span>
          </TabsTrigger>
          <TabsTrigger value="events" className="flex items-center space-x-2">
            <RiTimeLine className="w-4 h-4" />
            <span>事件 ({profile.recentEvents?.length || 0})</span>
          </TabsTrigger>
        </TabsList>

        {/* 属性标签页 */}
        <TabsContent value="properties" className="space-y-4">
          {/* 属性列表 - 新的布局格式 */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center justify-between">
                <span>设备属性</span>
                <span className="text-sm text-text-tertiary">
                  最后更新: {dayjs().format('HH:mm:ss')}
                </span>
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                {profile.properties?.map((property: DeviceProperty) => {
                  const isNumeric = isNumericProperty(property)
                  const chartData = isNumeric ? generateMiniChartData(property) : []
                  
                  // 根据属性状态确定图表颜色，使用主题颜色
                  let chartColor = 'var(--color-components-chart-line)' // 默认主题颜色
                  if (getPropertyStatusColor(property).includes('red')) {
                    chartColor = 'var(--color-text-destructive)'
                  } else if (getPropertyStatusColor(property).includes('green')) {
                    chartColor = 'var(--color-text-success)'
                  } else if (getPropertyStatusColor(property).includes('warning')) {
                    chartColor = 'var(--color-text-warning)'
                  }
                  
                  return (
                    <div 
                      key={property.id} 
                      className="flex items-center py-2 px-3 bg-components-panel-on-panel-item-bg rounded-lg hover:bg-components-panel-on-panel-item-bg-hover transition-colors"
                    >
                      {/* 状态圆形指示器 */}
                      <div 
                        className={`w-3 h-3 rounded-full flex-shrink-0 mr-3 ${getPropertyStatusColor(property)}`}
                        title={getPropertyStatusText(property)}
                      />
                      
                      {/* 属性ID - 自动宽度 */}
                      <span className="text-xs text-text-quaternary font-mono mr-3 min-w-0">
                        {property.name}
                      </span>
                      
                      {/* 属性名称 - 自动宽度 */}
                      <span className="text-sm font-medium text-text-secondary mr-4 min-w-0 flex-shrink-0">
                        {property.displayName }
                      </span>
                      
                      {/* 中间的弹性空间 */}
                      <div className="flex-1" />
                      
                      {/* 实时值 - 靠右排列 */}
                      <span className="text-sm font-semibold text-text-primary mr-2 flex-shrink-0">
                        {formatPropertyValue(property)}
                      </span>
                      
                      {/* 单位 - 靠右排列 */}
                      {getPropertyUnit(property) && (
                        <span className="text-xs text-text-tertiary mr-3 flex-shrink-0 w-8 text-right">
                          {getPropertyUnit(property)}
                        </span>
                      )}
                      
                      {/* 缩略曲线 - 仅数值类型显示 */}
                      {isNumeric && (
                        <div className="mr-2 flex-shrink-0">
                          <MiniChart data={chartData} color={chartColor} />
                        </div>
                      )}
                      
                      {/* 曲线按钮 - 仅数值类型显示 */}
                      {isNumeric && (
                        <Button
                          size="small"
                          variant="ghost"
                          onClick={() => openChartDialog(property)}
                          className="p-1 h-6 w-6 flex-shrink-0"
                          title="查看详细曲线"
                        >
                          <RiLineChartLine className="w-3 h-3" />
                        </Button>
                      )}
                    </div>
                  )
                })}
              </div>
              
              {(!profile.properties || profile.properties.length === 0) && (
                <div className="text-center py-8">
                  <RiPulseLine className="w-12 h-12 text-text-quaternary mx-auto mb-4" />
                  <h3 className="text-lg font-medium text-text-primary mb-2">暂无属性数据</h3>
                  <p className="text-text-tertiary">该设备还没有配置任何属性</p>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* 指令标签页 */}
        <TabsContent value="commands" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>设备指令</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {profile.commands?.map((command: DeviceCommand) => (
                  <div 
                    key={command.id} 
                    className="flex items-center justify-between py-3 px-4 bg-components-panel-on-panel-item-bg rounded-lg hover:bg-components-panel-on-panel-item-bg-hover transition-colors"
                  >
                    <div className="flex items-center space-x-4 flex-1">
                      {/* 命令ID */}
                      <span className="text-xs text-text-quaternary font-mono flex-shrink-0 w-16 truncate">
                        {command.id.slice(-12)}
                      </span>
                      
                      {/* 命令名称 */}
                      <div className="flex-1 min-w-0">
                        <div className="font-medium text-text-primary truncate">
                          {command.name}
                        </div>
                        {command.description && (
                          <div className="text-xs text-text-tertiary truncate">
                            {command.description}
                          </div>
                        )}
                      </div>
                    </div>
                    
                    {/* 执行按钮 */}
                    <Button 
                      size="small" 
                      onClick={() => openExecuteDialog(command)}
                      disabled={executeCommandMutation.isPending}
                      className="flex-shrink-0"
                    >
                      <RiPlayLine className="w-4 h-4 mr-1" />
                      执行
                    </Button>
                  </div>
                ))}
              </div>

              {(!profile.commands || profile.commands.length === 0) && (
                <div className="text-center py-8">
                  <RiTerminalBoxLine className="w-12 h-12 text-text-quaternary mx-auto mb-4" />
                  <h3 className="text-lg font-medium text-text-primary mb-2">暂无可用指令</h3>
                  <p className="text-text-tertiary">该设备还没有配置任何指令</p>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* 事件标签页 */}
        <TabsContent value="events" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>最近事件</CardTitle>
            </CardHeader>
            <CardContent>
              <DeviceEvents 
                events={profile.recentEvents || []} 
                isLoading={isLoading}
              />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* 执行指令确认对话框 */}
      <Modal
        isShow={isExecuteDialogOpen}
        onClose={() => setIsExecuteDialogOpen(false)}
        title={
          <div className="flex items-center">
            <RiSettings3Line className="w-5 h-5 mr-2" />
            执行指令确认
          </div>
        }
        className="!max-w-[600px]"
      >
        <div className="space-y-4 mt-4">
          {/* 指令基本信息 */}
          <div className="p-4 bg-state-accent-hover rounded-lg">
            <div className="flex items-center space-x-3 mb-2">
              <RiTerminalBoxLine className="w-5 h-5 text-text-accent" />
              <span className="font-medium text-text-primary">指令信息</span>
            </div>
            <div className="space-y-2 text-sm">
              <div className="flex items-center space-x-2">
                <span className="text-text-secondary">指令ID:</span>
                <span className="font-mono text-text-primary">{selectedCommand?.id}</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="text-text-secondary">指令名称:</span>
                <span className="font-medium text-text-primary">{selectedCommand?.name}</span>
              </div>
              {selectedCommand?.description && (
                <div className="flex items-start space-x-2">
                  <span className="text-text-secondary">描述:</span>
                  <span className="text-text-primary">{selectedCommand.description}</span>
                </div>
              )}
            </div>
          </div>
          
          {/* 参数输入区域 */}
          {selectedCommand && Object.keys(selectedCommand.parameters).length > 0 && (
            <div className="space-y-3">
              <div className="flex items-center space-x-2">
                <RiSettings3Line className="w-4 h-4 text-text-secondary" />
                <Label className="text-sm font-medium">参数设置</Label>
              </div>
              <div className="space-y-3 pl-6">
                {Object.entries(selectedCommand.parameters).map(([key, defaultValue]) => (
                  <div key={key} className="space-y-2">
                    <Label htmlFor={key} className="text-sm font-medium text-text-primary">
                      {key}
                      <span className="ml-2 text-xs text-text-tertiary">
                        ({typeof defaultValue})
                      </span>
                    </Label>
                    {typeof defaultValue === 'boolean' ? (
                      <select
                        id={key}
                        value={commandParams[key]?.toString() || defaultValue.toString()}
                        onChange={(e) => setCommandParams(prev => ({
                          ...prev,
                          [key]: e.target.value === 'true'
                        }))}
                        className="w-full px-3 py-2 border border-components-input-border-hover rounded-md focus:outline-none focus:ring-2 focus:ring-components-input-border-active focus:border-components-input-border-active bg-components-input-bg-normal text-text-primary"
                      >
                        <option value="true">是 (true)</option>
                        <option value="false">否 (false)</option>
                      </select>
                    ) : typeof defaultValue === 'number' ? (
                      <Input
                        id={key}
                        type="number"
                        value={commandParams[key] !== undefined ? commandParams[key] : defaultValue}
                        onChange={(e: React.ChangeEvent<HTMLInputElement>) => setCommandParams(prev => ({
                          ...prev,
                          [key]: Number(e.target.value)
                        }))}
                        placeholder={`默认值: ${defaultValue}`}
                      />
                    ) : (
                      <Input
                        id={key}
                        value={commandParams[key] !== undefined ? commandParams[key] : defaultValue}
                        onChange={(e: React.ChangeEvent<HTMLInputElement>) => setCommandParams(prev => ({
                          ...prev,
                          [key]: e.target.value
                        }))}
                        placeholder={`默认值: ${defaultValue}`}
                      />
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
          
          {/* 无参数提示 */}
          {selectedCommand && Object.keys(selectedCommand.parameters).length === 0 && (
            <div className="p-3 bg-components-panel-on-panel-item-bg rounded-lg text-center">
              <RiInformationLine className="w-5 h-5 text-text-quaternary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">此指令无需参数，点击确认执行即可</p>
            </div>
          )}
        </div>

        {/* Footer buttons */}
        <div className="flex justify-end space-x-2 mt-6">
          <Button 
            variant="secondary" 
            onClick={() => setIsExecuteDialogOpen(false)}
            disabled={executeCommandMutation.isPending}
          >
            取消
          </Button>
          <Button 
            onClick={handleExecuteCommand}
            disabled={executeCommandMutation.isPending}
            variant="primary"
          >
            {executeCommandMutation.isPending ? (
              <>
                <RiRefreshLine className="w-4 h-4 mr-2 animate-spin" />
                执行中...
              </>
            ) : (
              <>
                <RiPlayLine className="w-4 h-4 mr-2" />
                确认执行
              </>
            )}
          </Button>
        </div>
      </Modal>

      {/* 曲线查询对话框 */}
      <Modal
        isShow={isChartDialogOpen}
        onClose={() => setIsChartDialogOpen(false)}
        title={
          <div className="flex items-center">
            <RiBarChartBoxLine className="w-5 h-5 mr-2" />
            属性历史曲线: {selectedProperty?.displayName || selectedProperty?.name}
          </div>
        }
        className="!max-w-[800px]"
      >
        <div className="space-y-4 mt-4">
          {/* 属性信息 */}
          <div className="p-4 bg-state-accent-hover rounded-lg">
            <div className="flex items-center space-x-3 mb-2">
              <RiPulseLine className="w-5 h-5 text-text-accent" />
              <span className="font-medium text-text-primary">属性信息</span>
            </div>
            <div className="space-y-2 text-sm">
              <div className="flex items-center space-x-2">
                <span className="text-text-secondary">属性ID:</span>
                <span className="font-mono text-text-primary">{selectedProperty?.id}</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="text-text-secondary">属性名称:</span>
                <span className="font-medium text-text-primary">{selectedProperty?.displayName || selectedProperty?.name}</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="text-text-secondary">数据类型:</span>
                <span className="text-text-primary">{selectedProperty?.dataType}</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="text-text-secondary">当前值:</span>
                <span className="font-semibold text-text-primary">
                  {selectedProperty ? formatPropertyValue(selectedProperty) : '--'}
                  {selectedProperty && getPropertyUnit(selectedProperty) && (
                    <span className="ml-1 text-text-tertiary">{getPropertyUnit(selectedProperty)}</span>
                  )}
                </span>
              </div>
            </div>
          </div>

          {/* 时间范围选择 */}
          <div className="space-y-3">
            <Label className="text-sm font-medium text-text-primary">查询时间范围</Label>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <Label htmlFor="startDate" className="text-xs text-text-secondary">开始时间</Label>
                <Input
                  id="startDate"
                  type="datetime-local"
                  defaultValue={dayjs().subtract(1, 'day').format('YYYY-MM-DDTHH:mm')}
                />
              </div>
              <div>
                <Label htmlFor="endDate" className="text-xs text-text-secondary">结束时间</Label>
                <Input
                  id="endDate"
                  type="datetime-local"
                  defaultValue={dayjs().format('YYYY-MM-DDTHH:mm')}
                />
              </div>
            </div>
          </div>

          {/* 快捷时间选择 */}
          <div className="space-y-2">
            <Label className="text-sm font-medium text-text-primary">快捷选择</Label>
            <div className="flex flex-wrap gap-2">
              <Button size="small" variant="secondary">最近1小时</Button>
              <Button size="small" variant="secondary">最近6小时</Button>
              <Button size="small" variant="secondary">最近24小时</Button>
              <Button size="small" variant="secondary">最近7天</Button>
              <Button size="small" variant="secondary">最近30天</Button>
            </div>
          </div>

          {/* 曲线预览区域 */}
          <div className="space-y-2">
            <Label className="text-sm font-medium text-text-primary">曲线预览</Label>
            <div className="h-64 bg-components-panel-on-panel-item-bg rounded-lg flex items-center justify-center border-2 border-dashed border-divider-subtle">
              <div className="text-center">
                <RiLineChartLine className="w-12 h-12 text-text-quaternary mx-auto mb-2" />
                <p className="text-text-tertiary text-sm">曲线图表将在这里显示</p>
                <p className="text-text-quaternary text-xs mt-1">选择时间范围后点击刷新数据</p>
              </div>
            </div>
          </div>
        </div>

        {/* Footer buttons */}
        <div className="flex justify-end space-x-2 mt-6">
          <Button 
            variant="secondary" 
            onClick={() => setIsChartDialogOpen(false)}
          >
            关闭
          </Button>
          <Button variant="primary">
            <RiRefreshLine className="w-4 h-4 mr-2" />
            刷新数据
          </Button>
        </div>
      </Modal>
    </div>
  )
}

export default DeviceOverviewContent