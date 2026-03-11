'use client'
import type { FC } from 'react'
import { useUnmount } from 'ahooks'
import React, { useCallback, useEffect, useState } from 'react'
import { usePathname, useRouter } from 'next/navigation'
import {
  RiDashboard2Fill,
  RiDashboard2Line,
  RiFileList3Fill,
  RiFileList3Line,
  RiSettings3Fill,
  RiSettings3Line,
  RiBarChart2Fill,
  RiBarChart2Line,
  RiTerminalBoxFill,
  RiTerminalBoxLine,
  RiDatabase2Fill,
  RiDatabase2Line,
  RiAlarmWarningFill,
  RiAlarmWarningLine,
} from '@remixicon/react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import s from './style.module.css'
import cn from '@/utils/classnames'
import { useStore } from '@/app/components/device/store'
import DeviceDetailNav, { type NavIcon } from '@/app/components/app-sidebar'
import { useDevice } from '@/service/devices'
import { useAppContext } from '@/context/app-context'
import Loading from '@/app/components/base/loading'
import useBreakpoints from '@/hooks/use-breakpoints'
import { useStore as useTagStore } from '@/app/components/base/tag-management/store'
import dynamic from 'next/dynamic'

const TagManagementModal = dynamic(() => import('@/app/components/base/tag-management'), {
  ssr: false,
})

export type IDeviceDetailLayoutProps = {
  children: React.ReactNode
  deviceId: string
}

const DeviceDetailLayout: FC<IDeviceDetailLayoutProps> = (props) => {
  const {
    children,
    deviceId, // get deviceId in path
  } = props
  const router = useRouter()
  const pathname = usePathname()
  const media = useBreakpoints()
  const isMobile = media.isMobile
  const { isCurrentWorkspaceEditor } = useAppContext()
  const { deviceDetail, setDeviceDetail, setDeviceSidebarExpand } = useStore(useShallow(state => ({
    deviceDetail: state.deviceDetail,
    setDeviceDetail: state.setDeviceDetail,
    setDeviceSidebarExpand: state.setDeviceSidebarExpand,
  })))
  const showTagManagementModal = useTagStore(s => s.showTagManagementModal)
  const [deviceDetailRes, setDeviceDetailRes] = useState<any | null>(null)
  const [navigation, setNavigation] = useState<Array<{
    name: string
    href: string
    icon: NavIcon
    selectedIcon: NavIcon
  }>>([])

  const getNavigationConfig = useCallback((deviceId: string, isCurrentWorkspaceEditor: boolean) => {
    const navConfig = [
      {
        name: '概览',
        href: `#/${deviceId}/overview`,
        icon: RiDashboard2Line,
        selectedIcon: RiDashboard2Fill,
      },
      {
        name: '监控',
        href: `#/${deviceId}/monitoring`,
        icon: RiBarChart2Line,
        selectedIcon: RiBarChart2Fill,
      },
      ...(isCurrentWorkspaceEditor
        ? [{
          name: '配置',
          href: `#/${deviceId}/configuration`,
          icon: RiSettings3Line,
          selectedIcon: RiSettings3Fill,
        }]
        : []
      ),
    ]
    return navConfig
  }, [])

  useEffect(() => {
    if (deviceDetail) {
      const localeMode = localStorage.getItem('device-detail-collapse-or-expand') || 'expand'
      const mode = isMobile ? 'collapse' : 'expand'
      setDeviceSidebarExpand(isMobile ? mode : localeMode)
    }
  }, [deviceDetail, isMobile])

  const { data: deviceDetailData, isLoading: isLoadingDeviceDetail, error: deviceError } = useDevice(deviceId)

  useEffect(() => {
    if (deviceDetailData) {
      setDeviceDetailRes(deviceDetailData)
    }
  }, [deviceDetailData])

  useEffect(() => {
    if (deviceError) {
      console.error('Failed to load device:', deviceError)
      // 不要在这里处理401错误，让全局的auth-error事件处理器来处理
      // 这样可以避免在设备详情页面直接跳转到设备列表
    }
  }, [deviceError, router])

  useEffect(() => {
    if (!deviceDetailRes || isLoadingDeviceDetail)
      return
    const res = deviceDetailRes
    // redirection
    const canIEditDevice = isCurrentWorkspaceEditor
    if (!canIEditDevice && pathname.endsWith('configuration')) {
      router.replace(`/device/${deviceId}/overview`)
      return
    }
    
    // 如果访问的是已移除的页面，重定向到概览页面
    if (pathname.endsWith('properties') || pathname.endsWith('commands') || pathname.endsWith('events')) {
      router.replace(`/device/${deviceId}/overview`)
      return
    }
    
    setDeviceDetail({ ...res })
    setNavigation(getNavigationConfig(deviceId, isCurrentWorkspaceEditor))
  }, [deviceDetailRes, isCurrentWorkspaceEditor, isLoadingDeviceDetail, deviceId, pathname, router, setDeviceDetail, getNavigationConfig])

  useUnmount(() => {
    setDeviceDetail()
  })

  if (!deviceDetail) {
    return (
      <div className='flex h-full items-center justify-center bg-background-body'>
        <Loading />
      </div>
    )
  }

  return (
    <div className={cn(s.device, 'relative flex', 'overflow-hidden')}>
      {deviceDetail && (
        <DeviceDetailNav
          navigation={navigation}
        />
      )}
      <div className="grow overflow-hidden bg-components-panel-bg">
        {children}
      </div>
      {showTagManagementModal && (
        <TagManagementModal type='device' show={showTagManagementModal} />
      )}
    </div>
  )
}
export default React.memo(DeviceDetailLayout)