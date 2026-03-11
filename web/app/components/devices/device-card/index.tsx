/**
 * 设备卡片组件（重构版）
 * 拆分为多个子组件以提高可维护性
 */

'use client'

import React, { useCallback, useEffect, useState } from 'react'
import { useRouter } from 'next/navigation'
import { useTranslation } from 'react-i18next'
import { useDeleteDevice } from '@/service/devices'
import { getResourceTags } from '@/service/tag'
import { useAppContext } from '@/context/app-context'
import { useErrorHandler } from '@/hooks/use-error-handler'
import type { Device } from '@/types'
import type { Tag } from '@/app/components/base/tag-management/constant'
import { getDeviceStatus } from '@/lib/device-utils'
import dynamic from 'next/dynamic'

// 子组件
import DeviceHeader from './device-header'
import DeviceStatus from './device-status'
import DeviceContent from './device-content'
import DeviceActions from './device-actions'
import DeviceTags from './device-tags'
import StatusIcon from './status-icon'

const Confirm = dynamic(() => import('@/app/components/base/confirm'), {
  ssr: false,
})

export interface DeviceCardProps {
  device: Device
  onRefresh?: () => void
}

const DeviceCard: React.FC<DeviceCardProps> = ({ device, onRefresh }) => {
  const { t } = useTranslation('device')
  const { handleError } = useErrorHandler()
  const { isCurrentWorkspaceEditor } = useAppContext()
  const { push } = useRouter()

  const [showConfirmDelete, setShowConfirmDelete] = useState(false)
  const [tags, setTags] = useState<Tag[]>(device.tags || [])
  
  const deleteDeviceMutation = useDeleteDevice()
  const deviceStatus = getDeviceStatus(device.state)

  // 只在设备ID变化时加载标签（移除 handleError 依赖）
  useEffect(() => {
    const loadDeviceTags = async () => {
      try {
        const deviceTags = await getResourceTags(device.id)
        setTags(deviceTags)
      } catch (error) {
        // 标签加载失败不显示toast，静默处理
        console.error('Failed to load device tags:', error)
        setTags([])
      }
    }
    
    loadDeviceTags()
  }, [device.id]) // 只依赖 device.id

  // 当设备的tags属性更新时同步（但不触发API调用）
  useEffect(() => {
    if (device.tags) {
      setTags(device.tags)
    }
  }, [device.tags])

  // 删除设备
  const onConfirmDelete = useCallback(async () => {
    try {
      await deleteDeviceMutation.mutateAsync(device.id)
      if (onRefresh) onRefresh()
    } catch (error) {
      handleError(error, { context: 'Delete Device' })
    } finally {
      setShowConfirmDelete(false)
    }
  }, [device.id, deleteDeviceMutation, onRefresh, handleError])

  // 点击卡片跳转到设备详情（使用 hash 路由）
  const handleCardClick = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    // 直接跳转到 device-detail 页面并设置 hash
    window.location.href = `/device-detail#/${device.id}/overview`
  }, [device.id])

  return (
    <>
      <div
        onClick={handleCardClick}
        className='group relative col-span-1 inline-flex h-[160px] cursor-pointer flex-col rounded-xl border-[1px] border-solid border-components-card-border bg-components-card-bg shadow-sm transition-all duration-200 ease-in-out hover:shadow-lg'
      >
        {/* 头部：设备图标、名称、产品信息 */}
        <DeviceHeader 
          device={device} 
          statusIcon={<StatusIcon status={deviceStatus} />} 
        />

        {/* 状态徽章 */}
        <div className='absolute top-[14px] right-[14px]'>
          <DeviceStatus status={deviceStatus} />
        </div>

        {/* 内容：描述和属性 */}
        <DeviceContent device={device} />

        {/* 底部：标签和操作 */}
        {isCurrentWorkspaceEditor && (
          <div className='absolute bottom-1 left-0 right-0 flex h-[42px] shrink-0 items-center pb-[6px] pl-[14px] pr-[6px] pt-1'>
            <DeviceTags
              deviceId={device.id}
              tags={tags}
              onTagsUpdate={setTags}
              onRefresh={onRefresh}
            />
            
            <div className='mx-1 !hidden h-[14px] w-[1px] shrink-0 bg-divider-regular group-hover:!flex' />
            
            <DeviceActions
              deviceId={device.id}
              onDelete={() => setShowConfirmDelete(true)}
            />
          </div>
        )}
      </div>

      {/* 删除确认对话框 */}
      {showConfirmDelete && (
        <Confirm
          title={t('deleteDeviceConfirmTitle')}
          content={t('deleteDeviceConfirmContent')}
          isShow={showConfirmDelete}
          onConfirm={onConfirmDelete}
          onCancel={() => setShowConfirmDelete(false)}
        />
      )}
    </>
  )
}

export default React.memo(DeviceCard)
