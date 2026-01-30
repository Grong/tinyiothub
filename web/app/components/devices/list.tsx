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
} from '@remixicon/react'
import DeviceCard from './device-card'
import NewDeviceCard from './new-device-card'
import useDevicesQueryState from './hooks/use-devices-query-state'
import { useDevices } from '@/service/devices'
import TabSliderNew from '@/app/components/base/tab-slider-new'
import { useTabSearchParams } from '../../../hooks/use-tab-searchparams'
import Input from '@/app/components/base/input'
import { useStore as useTagStore } from '@/app/components/base/tag-management/store'
import TagFilter from '@/app/components/base/tag-management/filter'
import CheckboxWithLabel from '../base/checkbox'
import dynamic from 'next/dynamic'
import Empty from './empty'
import { useAppContext } from '@/context/app-context'

const TagManagementModal = dynamic(() => import('@/app/components/base/tag-management'), {
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
                <DeviceCard key={device.id} device={device} onRefresh={refetch} />
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
      </div>
    </>
  )
}

export default List