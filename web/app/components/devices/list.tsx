'use client'

import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { PAGINATION, DEBOUNCE_DELAY } from '@/lib/constants'
import { getDeviceStatus } from '@/lib/device-utils'
import { useDebounceFn } from 'ahooks'
import {
  RiApps2Line,
  RiWifiLine,
  RiWifiOffLine,
  RiSettings3Line,
  RiAlarmWarningLine,
  RiDeleteBinLine,
  RiPlayLine,
  RiStopLine,
} from '@remixicon/react'
import DeviceCard from './device-card'
import NewDeviceCard from './new-device-card'
import useDevicesQueryState from './hooks/use-devices-query-state'
import { 
  useDevices, 
  useBatchDeleteDevices, 
  useBatchEnableDevices, 
  useBatchDisableDevices 
} from '@/service/devices'
import TabSliderNew from '@/app/components/base/tab-slider-new'
import { useTabSearchParams } from '../../../hooks/use-tab-searchparams'
import Input from '@/app/components/base/input'
import { useStore as useTagStore } from '@/app/components/base/tag-management/store'
import TagFilter from '@/app/components/base/tag-management/filter'
import CheckboxWithLabel from '../base/checkbox'
import Checkbox from '../base/checkbox'
import Button from '../base/button'
import dynamic from 'next/dynamic'
import Empty from './empty'
import { useAppContext } from '@/context/app-context'
import type { Device } from '@/types'

const TagManagementModal = dynamic(() => import('@/app/components/base/tag-management'), {
  ssr: false,
})
const Confirm = dynamic(() => import('@/app/components/base/confirm'), {
  ssr: false,
})

// 将前端状态字符串转换为后端state数字
const getStateFromStatus = (status: string): number | undefined => {
  switch (status) {
    case 'online':
      return 1
    case 'error':
      return 2
    case 'maintenance':
      return 3
    case 'offline':
      return 0
    default:
      return undefined
  }
}

const List = () => {
  const { t } = useTranslation('device')
  const { isCurrentWorkspaceEditor } = useAppContext()
  const showTagManagementModal = useTagStore(s => s.showTagManagementModal)
  const [activeTab, setActiveTab] = useTabSearchParams({
    defaultTab: 'all',
  })
  const { query: { tagIDs = [], keywords = '', isCreatedByMe: queryIsCreatedByMe = false }, setQuery } = useDevicesQueryState()
  const [isCreatedByMe, setIsCreatedByMe] = useState(queryIsCreatedByMe)
  const [tagFilterValue, setTagFilterValue] = useState<string[]>(tagIDs)
  const [searchKeywords, setSearchKeywords] = useState(keywords)

  // 批量选择状态
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [batchConfirmType, setBatchConfirmType] = useState<'delete' | 'enable' | 'disable' | null>(null)

  // 批量操作 mutations
  const batchDeleteMutation = useBatchDeleteDevices()
  const batchEnableMutation = useBatchEnableDevices()
  const batchDisableMutation = useBatchDisableDevices()

  const setKeywords = useCallback((keywords: string) => {
    setQuery(prev => ({ ...prev, keywords }))
  }, [setQuery])
  const setTagIDs = useCallback((tagIDs: string[]) => {
    setQuery(prev => ({ ...prev, tagIDs }))
  }, [setQuery])

  // Tab选项配置
  const options = [
    { value: 'all', text: t('status.all'), icon: <RiApps2Line className='w-4 h-4' /> },
    { value: 'online', text: t('status.online'), icon: <RiWifiLine className='w-4 h-4' /> },
    { value: 'offline', text: t('status.offline'), icon: <RiWifiOffLine className='w-4 h-4' /> },
    { value: 'error', text: t('status.error'), icon: <RiAlarmWarningLine className='w-4 h-4' /> },
    { value: 'maintenance', text: t('status.maintenance'), icon: <RiSettings3Line className='w-4 h-4' /> },
  ]

  // 处理"仅显示我创建的设备"复选框变化
  const handleCreatedByMeChange = useCallback(() => {
    const newValue = !isCreatedByMe
    setIsCreatedByMe(newValue)
    setQuery(prev => ({ ...prev, isCreatedByMe: newValue }))
  }, [isCreatedByMe, setQuery])

  // 处理标签筛选变化
  const handleTagsChange = useCallback((tags: string[]) => {
    setTagFilterValue(tags)
    setTagIDs(tags)
  }, [setTagIDs])

  // 处理关键词搜索变化（带防抖）
  const { run: handleKeywordsChange } = useDebounceFn((value: string) => {
    setSearchKeywords(value)
    setKeywords(value)
  }, { wait: DEBOUNCE_DELAY.SEARCH })

  // 设备选择相关处理
  const handleSelectDevice = useCallback((deviceId: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev)
      if (next.has(deviceId)) {
        next.delete(deviceId)
      } else {
        next.add(deviceId)
      }
      return next
    })
  }, [])

  const handleSelectAll = useCallback(() => {
    if (selectedIds.size === devices.length) {
      setSelectedIds(new Set())
    } else {
      setSelectedIds(new Set(devices.map((d: Device) => d.id)))
    }
  }, [devices, selectedIds.size])

  const handleClearSelection = useCallback(() => {
    setSelectedIds(new Set())
  }, [])

  // 批量操作确认
  const handleBatchConfirm = useCallback(async () => {
    const ids = Array.from(selectedIds)
    try {
      if (batchConfirmType === 'delete') {
        await batchDeleteMutation.mutateAsync(ids)
      } else if (batchConfirmType === 'enable') {
        await batchEnableMutation.mutateAsync(ids)
      } else if (batchConfirmType === 'disable') {
        await batchDisableMutation.mutateAsync(ids)
      }
      setSelectedIds(new Set())
      refetch()
    } catch (error) {
      console.error('Batch operation failed:', error)
    } finally {
      setBatchConfirmType(null)
    }
  }, [batchConfirmType, selectedIds, batchDeleteMutation, batchEnableMutation, batchDisableMutation, refetch])

  const getBatchConfirmContent = () => {
    const count = selectedIds.size
    if (batchConfirmType === 'delete') {
      return t('batchDeleteConfirmContent', { count })
    } else if (batchConfirmType === 'enable') {
      return t('batchEnableConfirmContent', { count })
    } else if (batchConfirmType === 'disable') {
      return t('batchDisableConfirmContent', { count })
    }
    return ''
  }

  const isAllSelected = devices.length > 0 && selectedIds.size === devices.length
  const isIndeterminate = selectedIds.size > 0 && selectedIds.size < devices.length

  // 构建查询参数
  const queryParams = {
    page: PAGINATION.DEFAULT_PAGE,
    page_size: PAGINATION.DEVICE_LIST_PAGE_SIZE,
    name: searchKeywords,
    state: activeTab === 'all' ? undefined : getStateFromStatus(activeTab)?.toString(),
  }

  const { data, isLoading, error, refetch } = useDevices(queryParams)

  // 安全地获取设备列表
  const devices = Array.isArray(data) ? data : []
  const hasDevices = devices.length > 0

  return (
    <>
      <div className='flex h-full w-full flex-col overflow-hidden bg-background-body'>
        <div className='flex flex-wrap items-center justify-between gap-y-2 bg-background-body px-12 pb-2 pt-4 leading-[56px]'>
          <TabSliderNew
            value={activeTab}
            onChange={setActiveTab}
            options={options}
          />
          <div className='flex items-center gap-2'>
            <CheckboxWithLabel
              className='mr-2'
              label={t('showMyCreatedDevicesOnly')}
              isChecked={isCreatedByMe}
              onChange={handleCreatedByMeChange}
            />
            <TagFilter type='device' value={tagFilterValue} onChange={handleTagsChange} />
            <Input
              showLeftIcon
              showClearIcon
              wrapperClassName='w-[200px]'
              value={keywords}
              onChange={e => handleKeywordsChange(e.target.value)}
              onClear={() => handleKeywordsChange('')}
            />
          </div>
        </div>

        {/* 批量操作工具栏 */}
        {selectedIds.size > 0 && (
          <div className='flex items-center gap-4 bg-background-panel px-12 py-3 border-b border-divider-regular'>
            <div className='flex items-center gap-2'>
              <Checkbox
                checked={isAllSelected}
                indeterminate={isIndeterminate}
                onCheck={handleSelectAll}
              />
              <span className='text-sm text-text-secondary'>
                {t('selectedCount', { count: selectedIds.size })}
              </span>
            </div>
            <div className='flex items-center gap-2'>
              <Button
                size='small'
                variant='ghost'
                onClick={() => setBatchConfirmType('delete')}
                disabled={!isCurrentWorkspaceEditor}
              >
                <RiDeleteBinLine className='w-4 h-4 mr-1' />
                {t('batchDelete')}
              </Button>
              <Button
                size='small'
                variant='ghost'
                onClick={() => setBatchConfirmType('enable')}
                disabled={!isCurrentWorkspaceEditor}
              >
                <RiPlayLine className='w-4 h-4 mr-1' />
                {t('batchEnable')}
              </Button>
              <Button
                size='small'
                variant='ghost'
                onClick={() => setBatchConfirmType('disable')}
                disabled={!isCurrentWorkspaceEditor}
              >
                <RiStopLine className='w-4 h-4 mr-1' />
                {t('batchDisable')}
              </Button>
            </div>
            <Button size='small' variant='ghost' onClick={handleClearSelection}>
              {t('clearSelection')}
            </Button>
          </div>
        )}

        <div className='flex-1 overflow-y-auto px-12 pb-4'>
          {isLoading ? (
            <div className='flex items-center justify-center h-64'>
              <div className='text-text-tertiary'>{t('loading')}</div>
            </div>
          ) : error ? (
            <div className='flex items-center justify-center h-64'>
              <div className='text-text-destructive'>{t('loadFailed')}: {error.message}</div>
            </div>
          ) : hasDevices ? (
            <div className='grid grid-cols-1 gap-4 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-4 2xl:grid-cols-5 2k:grid-cols-6'>
              {isCurrentWorkspaceEditor
                && <NewDeviceCard onSuccess={refetch} />}
              {devices.map((device: any) => (
                <div key={device.id} className='relative'>
                  <DeviceCard 
                    device={device} 
                    onRefresh={refetch}
                    selected={selectedIds.has(device.id)}
                    onSelect={() => handleSelectDevice(device.id)}
                  />
                </div>
              ))}
            </div>
          ) : (
            <div className='grid grid-cols-1 gap-4 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-4 2xl:grid-cols-5 2k:grid-cols-6'>
              {isCurrentWorkspaceEditor
                && <NewDeviceCard onSuccess={refetch} />}
              <Empty />
            </div>
          )}
        </div>

        {showTagManagementModal && (
          <TagManagementModal type='device' show={showTagManagementModal} />
        )}

        {/* 批量操作确认对话框 */}
        <Confirm
          isShow={batchConfirmType !== null}
          type={batchConfirmType === 'delete' ? 'danger' : 'warning'}
          title={batchConfirmType === 'delete' ? t('batchDeleteConfirmTitle') : batchConfirmType === 'enable' ? t('batchEnableConfirmTitle') : t('batchDisableConfirmTitle')}
          content={getBatchConfirmContent()}
          confirmText={t('operation.confirm')}
          cancelText={t('operation.cancel')}
          onConfirm={handleBatchConfirm}
          onCancel={() => setBatchConfirmType(null)}
          isLoading={batchDeleteMutation.isPending || batchEnableMutation.isPending || batchDisableMutation.isPending}
        />
      </div>
    </>
  )
}

export default List