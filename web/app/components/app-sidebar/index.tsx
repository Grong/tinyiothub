import React, { useCallback, useEffect, useState } from 'react'
import { usePathname } from 'next/navigation'
import { useShallow } from 'zustand/react/shallow'
import NavLink from './navLink'
import type { NavIcon } from './navLink'

export type { NavIcon }
import DeviceInfo from '@/app/components/app-sidebar/device-info'
import DeviceSidebarDropdown from '@/app/components/app-sidebar/device-sidebar-dropdown'
import useBreakpoints from '@/hooks/use-breakpoints'
import { useStore as useDeviceStore } from '@/app/components/device/store'
import cn from '@/utils/classnames'
import Divider from '../base/divider'
import { useHover, useKeyPress } from 'ahooks'
import ToggleButton from '@/app/components/app-sidebar/toggle-button'
import { getKeyboardKeyCodeBySystem } from '../workflow/utils'

export type IDeviceDetailNavProps = {
  iconType?: 'device'
  navigation: Array<{
    name: string
    href: string
    icon: NavIcon
    selectedIcon: NavIcon
    disabled?: boolean
  }>
  extraInfo?: (modeState: string) => React.ReactNode
}

const DeviceDetailNav = ({
  navigation,
  extraInfo,
  iconType = 'device',
}: IDeviceDetailNavProps) => {
  const { deviceSidebarExpand, setDeviceSidebarExpand } = useDeviceStore(useShallow(state => ({
    deviceSidebarExpand: state.deviceSidebarExpand,
    setDeviceSidebarExpand: state.setDeviceSidebarExpand,
  })))
  const sidebarRef = React.useRef<HTMLDivElement>(null)
  const media = useBreakpoints()
  const isMobile = media.isMobile
  const expand = deviceSidebarExpand === 'expand'

  const handleToggle = useCallback(() => {
    setDeviceSidebarExpand(deviceSidebarExpand === 'expand' ? 'collapse' : 'expand')
  }, [deviceSidebarExpand, setDeviceSidebarExpand])

  const isHoveringSidebar = useHover(sidebarRef)

  // Check if the current path is a device configuration & fullscreen
  const pathname = usePathname()
  const inDeviceConfiguration = pathname.endsWith('/configuration')
  const deviceConfigurationMaximize = localStorage.getItem('device-configuration-maximize') === 'true'
  const [hideHeader, setHideHeader] = useState(deviceConfigurationMaximize)

  useEffect(() => {
    if (deviceSidebarExpand) {
      localStorage.setItem('device-detail-collapse-or-expand', deviceSidebarExpand)
      setDeviceSidebarExpand(deviceSidebarExpand)
    }
  }, [deviceSidebarExpand, setDeviceSidebarExpand])

  useKeyPress(`${getKeyboardKeyCodeBySystem()}.b`, (e) => {
    e.preventDefault()
    handleToggle()
  }, { exactMatch: true, useCapture: true })

  if (inDeviceConfiguration && hideHeader) {
    return (
      <div className='flex w-0 shrink-0'>
        <DeviceSidebarDropdown navigation={navigation} />
      </div>
    )
  }

  return (
    <div
      ref={sidebarRef}
      className={cn(
        'flex shrink-0 flex-col border-r border-divider-burn bg-background-default-subtle transition-all',
        expand ? 'w-[216px]' : 'w-14',
      )}
    >
      <div
        className={cn(
          'shrink-0',
          expand ? 'p-2' : 'p-1',
        )}
      >
        <DeviceInfo expand={expand} />
      </div>
      <div className='relative px-4 py-2'>
        <Divider
          type='horizontal'
          bgStyle={expand ? 'gradient' : 'solid'}
          className={cn(
            'my-0 h-px',
            expand
              ? 'bg-gradient-to-r from-divider-subtle to-background-gradient-mask-transparent'
              : 'bg-divider-subtle',
          )}
        />
        {!isMobile && isHoveringSidebar && (
          <ToggleButton
            className='absolute -right-3 top-[-3.5px] z-20'
            expand={expand}
            handleToggle={handleToggle}
          />
        )}
      </div>
      <nav
        className={cn(
          'flex grow flex-col gap-y-0.5',
          expand ? 'px-3 py-2' : 'p-3',
        )}
      >
        {navigation.map((item, index) => {
          return (
            <NavLink
              key={index}
              mode={deviceSidebarExpand}
              iconMap={{ selected: item.selectedIcon, normal: item.icon }}
              name={item.name}
              href={item.href}
              disabled={!!item.disabled}
            />
          )
        })}
      </nav>
      {extraInfo && extraInfo(deviceSidebarExpand)}
    </div>
  )
}

export default React.memo(DeviceDetailNav)