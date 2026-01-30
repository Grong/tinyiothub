'use client'

import React, { useState, useMemo, useCallback, useRef } from 'react'
import { useDebounceFn, useKeyPress } from 'ahooks'
import { useToastContext } from '@/app/components/base/toast'
import { useTemplates, useCreateDeviceFromTemplate, type ProcessedDeviceTemplate, type DeviceCreationInput, isFieldRequired } from '@/service/templates'
import { useDriverConfig } from '@/service/drivers'
import { useLocalizedText } from '@/utils/i18n-template'

export type WizardStep = 'template' | 'device'

interface WizardState {
  // 步骤状态
  currentStep: WizardStep
  selectedTemplate: ProcessedDeviceTemplate | null
  
  // 模板选择状态
  searchQuery: string
  templates: ProcessedDeviceTemplate[]
  filteredTemplates: ProcessedDeviceTemplate[]
  isLoading: boolean
  
  // 设备信息状态
  deviceName: string
  deviceDescription: string
  deviceAddress: string
  devicePosition: string
  selectedDriverName: string
  driverConfig: Record<string, string>
  
  // 验证状态
  validationErrors: Record<string, string>
  isFormValid: boolean
  
  // 创建状态
  isCreating: boolean
}

interface WizardActions {
  // 步骤控制
  setCurrentStep: (step: WizardStep) => void
  handleTemplateSelect: (template: ProcessedDeviceTemplate) => void
  handlePreviousStep: () => void
  
  // 模板选择
  setSearchQuery: (query: string) => void
  
  // 设备信息
  setDeviceName: (name: string) => void
  setDeviceDescription: (description: string) => void
  setDeviceAddress: (address: string) => void
  setDevicePosition: (position: string) => void
  setSelectedDriverName: (driverName: string) => void
  setDriverConfig: (config: Record<string, string>) => void
  
  // 验证
  validateForm: () => boolean
  clearValidationErrors: () => void
  
  // 创建设备
  handleCreateDevice: () => void
  
  // 重置状态
  resetState: () => void
}

interface UseWizardControllerProps {
  onSuccess: () => void
}

export const useWizardController = ({ onSuccess }: UseWizardControllerProps): [WizardState, WizardActions] => {
  const { notify } = useToastContext()
  const getLocalizedText = useLocalizedText()
  
  // 步骤状态
  const [currentStep, setCurrentStep] = useState<WizardStep>('template')
  const [selectedTemplate, setSelectedTemplate] = useState<ProcessedDeviceTemplate | null>(null)
  
  // 模板选择状态
  const [searchQuery, setSearchQuery] = useState('')
  const { data: templates = [], isLoading } = useTemplates()
  
  // 设备信息状态
  const [deviceName, setDeviceName] = useState('')
  const [deviceDescription, setDeviceDescription] = useState('')
  const [deviceAddress, setDeviceAddress] = useState('')
  const [devicePosition, setDevicePosition] = useState('')
  const [selectedDriverName, setSelectedDriverName] = useState('')
  const [driverConfig, setDriverConfig] = useState<Record<string, string>>({})
  
  // 验证状态
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({})
  
  // 获取当前选中驱动的配置参数
  const { data: driverConfigData } = useDriverConfig(
    selectedDriverName,
    !!selectedDriverName
  )
  
  // 创建状态
  const isCreatingRef = useRef(false)
  const createDeviceMutation = useCreateDeviceFromTemplate()

  // 验证函数
  const validateForm = useCallback((): boolean => {
    const errors: Record<string, string> = {}
    
    // 验证设备名称
    if (!deviceName.trim()) {
      errors.deviceName = '设备名称不能为空'
    } else if (deviceName.trim().length < 2) {
      errors.deviceName = '设备名称至少需要2个字符'
    } else if (deviceName.trim().length > 50) {
      errors.deviceName = '设备名称不能超过50个字符'
    }
    
    // 验证设备地址（如果模板要求必填）
    if (selectedTemplate && isFieldRequired(selectedTemplate.deviceInfo, 'address') && !deviceAddress.trim()) {
      errors.deviceAddress = '设备地址是必填字段'
    }
    
    // 验证驱动配置
    if (selectedDriverName && driverConfigData?.configOptions) {
      for (const option of driverConfigData.configOptions) {
        if (option.required) {
          const userValue = driverConfig[option.name]
          
          // 检查用户输入值或默认值
          const hasUserValue = userValue !== undefined && userValue.trim() !== ''
          const hasDefaultValue = option.defaultValue && option.defaultValue.trim() !== ''
          
          if (!hasUserValue && !hasDefaultValue) {
            errors[`driverConfig.${option.name}`] = `${option.label}是必填字段`
          }
        }
      }
    }
    
    setValidationErrors(errors)
    return Object.keys(errors).length === 0
  }, [selectedTemplate, deviceName, deviceAddress, selectedDriverName, driverConfig, driverConfigData])

  // 清除验证错误
  const clearValidationErrors = useCallback(() => {
    setValidationErrors({})
  }, [])

  // 计算表单是否有效 - 实时计算，不依赖validationErrors状态
  const isFormValid = useMemo(() => {
    // 基本验证：设备名称
    if (!deviceName.trim() || deviceName.trim().length < 2 || deviceName.trim().length > 50) {
      return false
    }
    
    // 验证设备地址（如果模板要求必填）
    if (selectedTemplate && isFieldRequired(selectedTemplate.deviceInfo, 'address') && !deviceAddress.trim()) {
      return false
    }
    
    // 验证驱动配置
    if (selectedDriverName && driverConfigData?.configOptions) {
      for (const option of driverConfigData.configOptions) {
        if (option.required) {
          const userValue = driverConfig[option.name]
          
          // 检查用户输入值或默认值
          const hasUserValue = userValue !== undefined && userValue.trim() !== ''
          const hasDefaultValue = option.defaultValue && option.defaultValue.trim() !== ''
          
          if (!hasUserValue && !hasDefaultValue) {
            return false
          }
        }
      }
    }
    
    return true
  }, [deviceName, deviceAddress, selectedTemplate, selectedDriverName, driverConfig, driverConfigData])

  // 过滤模板
  const filteredTemplates = useMemo(() => {
    if (!searchQuery.trim()) {
      return templates
    }

    const query = searchQuery.toLowerCase()
    return templates.filter(template => {
      const name = template.name?.toLowerCase() || ''
      const displayName = typeof template.displayName === 'object' && template.displayName
        ? Object.values(template.displayName).join(' ').toLowerCase()
        : String(template.displayName || '').toLowerCase()
      const description = typeof template.description === 'object' && template.description
        ? Object.values(template.description).join(' ').toLowerCase()
        : String(template.description || '').toLowerCase()
      
      return name.includes(query) || displayName.includes(query) || description.includes(query)
    })
  }, [templates, searchQuery])

  // 处理模板选择
  const handleTemplateSelect = useCallback((template: ProcessedDeviceTemplate) => {
    setSelectedTemplate(template)
    
    // 使用模板的默认值填充设备信息
    const defaultName = template.deviceInfo?.defaultNamePattern 
      ? template.deviceInfo.defaultNamePattern.replace('{name}', template.name)
      : template.name
    setDeviceName(defaultName)
    
    const defaultDesc = template.deviceInfo?.defaultDescription
      ? getLocalizedText(template.deviceInfo.defaultDescription, '')
      : getLocalizedText(template.description || {}, '')
    setDeviceDescription(defaultDesc)
    
    setDeviceAddress('')
    setDevicePosition(template.deviceInfo?.defaultPosition || '')
    
    // 设置默认驱动
    setSelectedDriverName(template.driverName || '')
    
    // 重置驱动配置
    setDriverConfig({})
    
    // 进入下一步
    setCurrentStep('device')
  }, [getLocalizedText])

  // 返回上一步
  const handlePreviousStep = useCallback(() => {
    setCurrentStep('template')
  }, [])

  // 创建设备
  const onCreate = useCallback(async () => {
    if (!selectedTemplate) {
      notify({ type: 'error', message: '请先选择设备模板' })
      return
    }
    
    // 使用统一的表单验证
    if (!validateForm()) {
      notify({ type: 'error', message: '请检查并修正表单中的错误' })
      return
    }
    
    if (isCreatingRef.current) return
    isCreatingRef.current = true

    try {
      // 构建完整的驱动配置，包含默认值
      const finalDriverConfig: Record<string, string> = {}
      
      if (selectedDriverName && driverConfigData?.configOptions) {
        for (const option of driverConfigData.configOptions) {
          const userValue = driverConfig[option.name]
          
          // 如果用户有输入值，使用用户值
          if (userValue !== undefined && userValue !== '') {
            finalDriverConfig[option.name] = userValue
          }
          // 如果用户没有输入值但有默认值，使用默认值
          else if (option.defaultValue) {
            finalDriverConfig[option.name] = option.defaultValue
          }
        }
      }
      
      const deviceInput: DeviceCreationInput = {
        name: deviceName.trim(),
        displayName: deviceName.trim(),
        description: deviceDescription.trim() || undefined,
        address: deviceAddress.trim() || undefined,
        position: devicePosition.trim() || undefined,
        driverName: selectedDriverName || undefined,
        driverOptions: Object.keys(finalDriverConfig).length > 0 ? JSON.stringify(finalDriverConfig) : undefined,
        propertyValues: {},
        enabledCommands: selectedTemplate.commands?.map(cmd => cmd.name) || [],
      }

      await createDeviceMutation.mutateAsync({
        templateId: selectedTemplate.id,
        deviceInput,
      })

      notify({ type: 'success', message: '设备创建成功' })
      onSuccess()
    } catch (error: any) {
      // 提取更详细的错误信息
      let errorMessage = '设备创建失败'
      
      // 优先使用 error.message（已经由 fetch.ts 处理过的错误信息）
      if (error.message) {
        errorMessage = error.message
      } else if (error.data?.msg) {
        errorMessage = error.data.msg
      } else if (error.data?.message) {
        errorMessage = error.data.message
      }
      
      notify({
        type: 'error',
        message: errorMessage,
      })
    } finally {
      isCreatingRef.current = false
    }
  }, [selectedTemplate, deviceName, deviceDescription, deviceAddress, devicePosition, selectedDriverName, driverConfig, validateForm, createDeviceMutation, notify, onSuccess])

  const { run: handleCreateDevice } = useDebounceFn(onCreate, { wait: 300 })
  
  // 键盘快捷键
  useKeyPress(['meta.enter', 'ctrl.enter'], () => {
    if (currentStep === 'device' && selectedTemplate && deviceName.trim()) {
      handleCreateDevice()
    }
  })

  // 重置状态
  const resetState = useCallback(() => {
    setCurrentStep('template')
    setSelectedTemplate(null)
    setSearchQuery('')
    setDeviceName('')
    setDeviceDescription('')
    setDeviceAddress('')
    setDevicePosition('')
    setSelectedDriverName('')
    setDriverConfig({})
    setValidationErrors({})
  }, [])

  const state: WizardState = {
    currentStep,
    selectedTemplate,
    searchQuery,
    templates,
    filteredTemplates,
    isLoading,
    deviceName,
    deviceDescription,
    deviceAddress,
    devicePosition,
    selectedDriverName,
    driverConfig,
    validationErrors,
    isFormValid,
    isCreating: createDeviceMutation.isPending,
  }

  const actions: WizardActions = {
    setCurrentStep,
    handleTemplateSelect,
    handlePreviousStep,
    setSearchQuery,
    setDeviceName,
    setDeviceDescription,
    setDeviceAddress,
    setDevicePosition,
    setSelectedDriverName,
    setDriverConfig,
    validateForm,
    clearValidationErrors,
    handleCreateDevice,
    resetState,
  }

  return [state, actions]
}