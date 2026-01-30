'use client'

import React from 'react'
import Input from '@/app/components/base/input'
import Textarea from '@/app/components/base/textarea'
import Divider from '@/app/components/base/divider'
import {
  PortalToFollowElem,
  PortalToFollowElemContent,
  PortalToFollowElemTrigger,
} from '@/app/components/base/portal-to-follow-elem'
import { RiArrowDownSLine, RiCheckLine } from '@remixicon/react'
import { type ProcessedDeviceTemplate, isFieldRequired } from '@/service/templates'
import { useLocalizedText } from '@/utils/i18n-template'
import { useDriverConfig, useDriversList, type DriverConfigOption } from '@/service/drivers'
import cn from '@/utils/classnames'

interface DeviceInfoStepProps {
  selectedTemplate: ProcessedDeviceTemplate
  deviceName: string
  onDeviceNameChange: (name: string) => void
  deviceDescription: string
  onDeviceDescriptionChange: (description: string) => void
  deviceAddress: string
  onDeviceAddressChange: (address: string) => void
  devicePosition: string
  onDevicePositionChange: (position: string) => void
  selectedDriverName: string
  onDriverNameChange: (driverName: string) => void
  driverConfig: Record<string, string>
  onDriverConfigChange: (config: Record<string, string>) => void
  validationErrors?: Record<string, string>
}

// 获取分类图标
const getCategoryIcon = (category: string) => {
  const icons: Record<string, string> = {
    sensors: '🌡️',
    controllers: '🎛️',
    cameras: '📷',
    gateways: '🌐',
    default: '📦',
  }
  return icons[category] || icons.default
}

// 驱动选择下拉框组件
const DriverSelect: React.FC<{
  value: string
  onChange: (value: string) => void
  options: Array<{ name: string; description?: string }>
  isLoading: boolean
  placeholder?: string
}> = ({ value, onChange, options, isLoading, placeholder = "请选择驱动" }) => {
  const [open, setOpen] = React.useState(false)
  
  const selectedOption = options.find(option => option.name === value)
  const displayText = selectedOption 
    ? `${selectedOption.name}${selectedOption.description ? ` - ${selectedOption.description}` : ''}`
    : placeholder

  if (isLoading) {
    return (
      <div className="flex h-9 items-center justify-center rounded-lg border-0 bg-components-input-bg-normal px-3">
        <div className="text-sm text-text-tertiary">加载驱动列表...</div>
      </div>
    )
  }

  return (
    <PortalToFollowElem
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      offset={4}
      triggerPopupSameWidth
    >
      <PortalToFollowElemTrigger onClick={() => setOpen(!open)} asChild>
        <div className={cn(
          'system-sm-regular group flex h-9 items-center rounded-lg bg-components-input-bg-normal px-3 text-components-input-text-filled cursor-pointer hover:bg-state-base-hover-alt',
          open && 'bg-state-base-hover-alt',
          !selectedOption && 'text-components-input-text-placeholder'
        )}>
          <div className="grow truncate" title={displayText}>
            {displayText}
          </div>
          <RiArrowDownSLine className={cn(
            'h-4 w-4 shrink-0 text-text-quaternary group-hover:text-text-secondary',
            open && 'text-text-secondary'
          )} />
        </div>
      </PortalToFollowElemTrigger>
      <PortalToFollowElemContent className="z-[9999]">
        <div className="max-h-80 overflow-auto rounded-xl border-[0.5px] border-components-panel-border bg-components-panel-bg-blur p-1 shadow-lg">
          <div
            className="system-sm-medium flex h-8 cursor-pointer items-center rounded-lg px-2 text-text-secondary hover:bg-state-base-hover"
            onClick={() => {
              onChange('')
              setOpen(false)
            }}
          >
            <div className="mr-1 grow truncate px-1 text-components-input-text-placeholder">
              {placeholder}
            </div>
            {!value && <RiCheckLine className="h-4 w-4 shrink-0 text-text-accent" />}
          </div>
          {options.map((option) => (
            <div
              key={option.name}
              className="system-sm-medium flex h-8 cursor-pointer items-center rounded-lg px-2 text-text-secondary hover:bg-state-base-hover"
              title={`${option.name}${option.description ? ` - ${option.description}` : ''}`}
              onClick={() => {
                onChange(option.name)
                setOpen(false)
              }}
            >
              <div className="mr-1 grow truncate px-1">
                {option.name}
                {option.description && (
                  <span className="text-text-tertiary">
                    {` - ${option.description.length > 30 ? option.description.substring(0, 30) + '...' : option.description}`}
                  </span>
                )}
              </div>
              {value === option.name && <RiCheckLine className="h-4 w-4 shrink-0 text-text-accent" />}
            </div>
          ))}
        </div>
      </PortalToFollowElemContent>
    </PortalToFollowElem>
  )
}

// 布尔值选择下拉框组件
const BooleanSelect: React.FC<{
  value: string
  onChange: (value: string) => void
}> = ({ value, onChange }) => {
  const [open, setOpen] = React.useState(false)
  
  const options = [
    { label: '是', value: 'true' },
    { label: '否', value: 'false' }
  ]
  
  const selectedOption = options.find(option => option.value === value)
  const displayText = selectedOption?.label || '请选择'

  return (
    <PortalToFollowElem
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      offset={4}
      triggerPopupSameWidth
    >
      <PortalToFollowElemTrigger onClick={() => setOpen(!open)} asChild>
        <div className={cn(
          'system-sm-regular group flex h-9 items-center rounded-lg bg-components-input-bg-normal px-3 text-components-input-text-filled cursor-pointer hover:bg-state-base-hover-alt',
          open && 'bg-state-base-hover-alt',
          !selectedOption && 'text-components-input-text-placeholder'
        )}>
          <div className="grow" title={displayText}>
            {displayText}
          </div>
          <RiArrowDownSLine className={cn(
            'h-4 w-4 shrink-0 text-text-quaternary group-hover:text-text-secondary',
            open && 'text-text-secondary'
          )} />
        </div>
      </PortalToFollowElemTrigger>
      <PortalToFollowElemContent className="z-[9999]">
        <div className="max-h-80 overflow-auto rounded-xl border-[0.5px] border-components-panel-border bg-components-panel-bg-blur p-1 shadow-lg">
          {options.map((option) => (
            <div
              key={option.value}
              className="system-sm-medium flex h-8 cursor-pointer items-center rounded-lg px-2 text-text-secondary hover:bg-state-base-hover"
              onClick={() => {
                onChange(option.value)
                setOpen(false)
              }}
            >
              <div className="mr-1 grow px-1">
                {option.label}
              </div>
              {value === option.value && <RiCheckLine className="h-4 w-4 shrink-0 text-text-accent" />}
            </div>
          ))}
        </div>
      </PortalToFollowElemContent>
    </PortalToFollowElem>
  )
}

// 渲染驱动配置字段
const renderDriverConfigField = (
  option: DriverConfigOption,
  value: string,
  onChange: (name: string, value: string) => void
) => {
  const handleChange = (newValue: string) => {
    onChange(option.name, newValue)
  }

  const placeholder = option.defaultValue 
    ? `默认: ${option.defaultValue}`
    : `请输入${option.label}`

  switch (option.type) {
    case 'number':
      return (
        <Input
          type="number"
          value={value}
          onChange={(e) => handleChange(e.target.value)}
          placeholder={placeholder}
        />
      )
    case 'boolean':
      return (
        <BooleanSelect
          value={value}
          onChange={handleChange}
        />
      )
    case 'select':
      // 如果有选项列表，渲染下拉框（这里需要扩展 DriverConfigOption 类型来支持选项）
      return (
        <Input
          value={value}
          onChange={(e) => handleChange(e.target.value)}
          placeholder={placeholder}
        />
      )
    default:
      return (
        <Input
          value={value}
          onChange={(e) => handleChange(e.target.value)}
          placeholder={placeholder}
        />
      )
  }
}

const DeviceInfoStep: React.FC<DeviceInfoStepProps> = ({
  selectedTemplate,
  deviceName,
  onDeviceNameChange,
  deviceDescription,
  onDeviceDescriptionChange,
  deviceAddress,
  onDeviceAddressChange,
  devicePosition,
  onDevicePositionChange,
  selectedDriverName,
  onDriverNameChange,
  driverConfig,
  onDriverConfigChange,
  validationErrors = {},
}) => {
  const getLocalizedText = useLocalizedText()
  const templateDisplayName = getLocalizedText(selectedTemplate.displayName || {}, selectedTemplate.name)

  // 获取可用驱动列表
  const { data: driversData, isLoading: isLoadingDrivers } = useDriversList()
  const availableDrivers = driversData || []

  // 获取当前选中驱动的配置参数
  const { data: driverConfigData, isLoading: isLoadingDriverConfig } = useDriverConfig(
    selectedDriverName,
    !!selectedDriverName
  )

  // 验证状态 - 使用从父组件传入的验证错误
  const hasError = (fieldName: string) => Boolean(validationErrors[fieldName])
  const getError = (fieldName: string) => validationErrors[fieldName] || ''

  // 处理驱动选择变更
  const handleDriverChange = (newDriverName: string) => {
    onDriverNameChange(newDriverName)
    // 重置驱动配置
    onDriverConfigChange({})
  }

  // 处理驱动配置变更
  const handleDriverConfigChange = (name: string, value: string) => {
    onDriverConfigChange({
      ...driverConfig,
      [name]: value,
    })
  }

  return (
    <div className="space-y-4 overflow-hidden">
      <div className="mb-2 leading-6">
        <span className="system-sm-semibold text-text-secondary">填写设备信息</span>
      </div>

      {/* 选中的模板信息 */}
      <div className="rounded-xl border border-components-panel-border bg-components-panel-on-panel-item-bg p-4">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-components-button-secondary-bg text-xl">
            {getCategoryIcon(selectedTemplate.category)}
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-sm font-semibold text-text-secondary">
              {templateDisplayName}
            </h3>
            <div className="mt-0.5 text-xs text-text-tertiary break-words">
              {selectedTemplate.manufacturer && (
                <span className="inline-block">{selectedTemplate.manufacturer} • </span>
              )}
              <span className="inline-block">{selectedTemplate.deviceType || selectedTemplate.category}</span>
              {selectedTemplate.version && (
                <span className="inline-block"> • v{selectedTemplate.version}</span>
              )}
              {selectedDriverName && (
                <span className="inline-block"> • {selectedDriverName}</span>
              )}
            </div>
          </div>
          {selectedTemplate.isBuiltin && (
            <div className="system-2xs-medium-uppercase rounded bg-background-section px-2 py-1 text-text-tertiary">
              内置
            </div>
          )}
        </div>
      </div>

      <Divider style={{ margin: 0 }} />

      {/* 设备基本信息表单 */}
      <div>
        <div className="mb-1 flex h-6 items-center">
          <label className="system-sm-semibold text-text-secondary">设备名称</label>
          <span className="system-xs-regular ml-1 text-text-accent-red">*</span>
        </div>
        <Input
          value={deviceName}
          onChange={(e) => onDeviceNameChange(e.target.value)}
          placeholder="请输入设备名称"
          className={hasError('deviceName') ? 'border-components-input-border-error' : ''}
        />
        {hasError('deviceName') && (
          <div className="mt-1 text-xs text-text-destructive">
            {getError('deviceName')}
          </div>
        )}
      </div>

      <div>
        <div className="mb-1 flex h-6 items-center">
          <label className="system-sm-semibold text-text-secondary">设备描述</label>
          <span className="system-xs-regular ml-1 text-text-tertiary">
            (可选)
          </span>
        </div>
        <Textarea
          className="resize-none"
          placeholder="请输入设备描述"
          value={deviceDescription}
          onChange={(e) => onDeviceDescriptionChange(e.target.value)}
        />
      </div>

      <div>
        <div className="mb-1 flex h-6 items-center">
          <label className="system-sm-semibold text-text-secondary">设备地址</label>
          {isFieldRequired(selectedTemplate.deviceInfo, 'address') ? (
            <span className="system-xs-regular ml-1 text-text-accent-red">*</span>
          ) : (
            <span className="system-xs-regular ml-1 text-text-tertiary">(可选)</span>
          )}
        </div>
        <Input
          value={deviceAddress}
          onChange={(e) => onDeviceAddressChange(e.target.value)}
          placeholder="请输入设备IP地址或连接地址"
          className={hasError('deviceAddress') ? 'border-components-input-border-error' : ''}
        />
        {hasError('deviceAddress') && (
          <div className="mt-1 text-xs text-text-destructive">
            {getError('deviceAddress')}
          </div>
        )}
      </div>

      <div>
        <div className="mb-1 flex h-6 items-center">
          <label className="system-sm-semibold text-text-secondary">安装位置</label>
          <span className="system-xs-regular ml-1 text-text-tertiary">
            (可选)
          </span>
        </div>
        <Input
          value={devicePosition}
          onChange={(e) => onDevicePositionChange(e.target.value)}
          placeholder="请输入设备安装位置"
        />
      </div>

      <Divider style={{ margin: 0 }} />

      {/* 驱动选择 */}
      <div>
        <div className="mb-1 flex h-6 items-center">
          <label className="system-sm-semibold text-text-secondary">设备驱动</label>
          <span className="system-xs-regular ml-1 text-text-tertiary">
            (选择适合的驱动程序)
          </span>
        </div>
        <DriverSelect
          value={selectedDriverName}
          onChange={handleDriverChange}
          options={availableDrivers}
          isLoading={isLoadingDrivers}
          placeholder="请选择驱动"
        />
        {selectedTemplate.driverName && selectedDriverName !== selectedTemplate.driverName && (
          <div className="mt-1 text-xs text-text-tertiary">
            模板默认驱动: {selectedTemplate.driverName}
          </div>
        )}
      </div>

      {/* 驱动配置参数 */}
      {selectedDriverName && (
        <>
          <Divider style={{ margin: 0 }} />
          <div>
            <div className="mb-3 flex h-6 items-center">
              <label className="system-sm-semibold text-text-secondary">驱动配置</label>
              <span className="system-xs-regular ml-1 text-text-tertiary">
                ({selectedDriverName})
              </span>
            </div>
            
            {isLoadingDriverConfig ? (
              <div className="flex items-center justify-center py-4">
                <div className="text-sm text-text-tertiary">加载驱动配置参数...</div>
              </div>
            ) : driverConfigData?.configOptions && driverConfigData.configOptions.length > 0 ? (
              <div className="space-y-3">
                {driverConfigData.configOptions.map((option) => (
                  <div key={option.name}>
                    <div className="mb-1 flex h-6 items-center">
                      <label className="system-sm-semibold text-text-secondary">
                        {option.label}
                      </label>
                      {!option.required && (
                        <span className="system-xs-regular ml-1 text-text-tertiary">
                          (可选)
                        </span>
                      )}
                      {option.required && (
                        <span className="system-xs-regular ml-1 text-text-accent-red">
                          *
                        </span>
                      )}
                      {option.defaultValue && (
                        <span className="system-xs-regular ml-2 text-text-tertiary">
                          • 默认: {option.defaultValue}
                        </span>
                      )}
                    </div>
                    {renderDriverConfigField(
                      option,
                      driverConfig[option.name] || '',
                      handleDriverConfigChange
                    )}
                    {hasError(`driverConfig.${option.name}`) && (
                      <div className="mt-1 text-xs text-text-destructive">
                        {getError(`driverConfig.${option.name}`)}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            ) : (
              <div className="rounded-md border border-components-panel-border bg-components-panel-on-panel-item-bg p-3">
                <div className="text-sm text-text-tertiary">
                  该驱动无需额外配置参数
                </div>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}

export default DeviceInfoStep