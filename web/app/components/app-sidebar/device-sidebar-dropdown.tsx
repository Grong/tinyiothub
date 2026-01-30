import React, { useCallback, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppContext } from '@/context/app-context'
import {
  RiEqualizer2Line,
  RiMenuLine,
} from '@remixicon/react'
import {
  PortalToFollowElem,
  PortalToFollowElemContent,
  PortalToFollowElemTrigger,
} from '@/app/components/base/portal-to-follow-elem'
import Divider from '../base/divider'
import DeviceInfo from './device-info'
import NavLink from './navLink'
import { useStore as useDeviceStore } from '@/app/components/device/store'
import type { NavIcon } from './navLink'
import cn from '@/utils/classnames'

type Props = {
  navigation: Array<{
    name: string
    href: string
    icon: NavIcon
    selectedIcon: NavIcon
  }>
}

const DeviceSidebarDropdown = ({ navigation }: Props) => {
  const { t } = useTranslation('common')
  const { isCurrentWorkspaceEditor } = useAppContext()
  const deviceDetail = useDeviceStore(state => state.deviceDetail)
  const [detailExpand, setDetailExpand] = useState(false)

  const [open, doSetOpen] = useState(false)
  const openRef = useRef(open)
  const setOpen = useCallback((v: boolean) => {
    doSetOpen(v)
    openRef.current = v
  }, [doSetOpen])
  const handleTrigger = useCallback(() => {
    setOpen(!openRef.current)
  }, [setOpen])

  if (!deviceDetail)
    return null

  const getDeviceStatusText = (state?: number) => {
    switch (state) {
      case 1:
        return t('device.status.online')
      case 2:
        return t('device.status.error')
      case 3:
        return t('device.status.maintenance')
      case 0:
      default:
        return t('device.status.offline')
    }
  }

  return (
    <>
      <div className='fixed left-2 top-2 z-20'>
        <PortalToFollowElem
          open={open}
          onOpenChange={setOpen}
          placement='bottom-start'
          offset={{
            mainAxis: -41,
          }}
        >
          <PortalToFollowElemTrigger onClick={handleTrigger}>
            <div className={cn('flex cursor-pointer items-center rounded-[10px] border-[0.5px] border-components-actionbar-border bg-components-actionbar-bg p-1 shadow-lg backdrop-blur-sm hover:bg-background-default-hover', open && 'bg-background-default-hover')}>
              <div className='flex h-8 w-8 items-center justify-center rounded-lg border border-divider-subtle'>
                <RiEqualizer2Line className='h-4 w-4 text-text-tertiary' />
              </div>
              <RiMenuLine className='h-4 w-4 text-text-tertiary' />
            </div>
          </PortalToFollowElemTrigger>
          <PortalToFollowElemContent className='z-[1000]'>
            <div className={cn('w-[305px] rounded-xl border-[0.5px] border-components-panel-border bg-background-default-subtle shadow-lg')}>
              <div className='p-2'>
                <div
                  className={cn('flex flex-col gap-2 rounded-lg p-2 pb-2.5', isCurrentWorkspaceEditor && 'cursor-pointer hover:bg-state-base-hover')}
                  onClick={() => {
                    setDetailExpand(true)
                    setOpen(false)
                  }}
                >
                  <div className='flex items-center justify-between self-stretch'>
                    <div className='flex h-10 w-10 items-center justify-center rounded-lg border border-divider-subtle'>
                      <RiEqualizer2Line className='h-6 w-6 text-text-tertiary' />
                    </div>
                    <div className='flex items-center justify-center rounded-md p-0.5'>
                      <div className='flex h-5 w-5 items-center justify-center'>
                        <RiEqualizer2Line className='h-4 w-4 text-text-tertiary' />
                      </div>
                    </div>
                  </div>
                  <div className='flex flex-col items-start gap-1'>
                    <div className='flex w-full'>
                      <div className='system-md-semibold truncate text-text-secondary'>{deviceDetail.name}</div>
                    </div>
                    <div className='system-2xs-medium-uppercase text-text-tertiary'>{getDeviceStatusText(deviceDetail.state)}</div>
                  </div>
                </div>
              </div>
              <div className='px-4'>
                <Divider bgStyle='gradient' />
              </div>
              <nav className='space-y-0.5 px-3 pb-6 pt-4'>
                {navigation.map((item, index) => {
                  return (
                    <NavLink key={index} mode='expand' iconMap={{ selected: item.selectedIcon, normal: item.icon }} name={item.name} href={item.href} />
                  )
                })}
              </nav>
            </div>
          </PortalToFollowElemContent>
        </PortalToFollowElem>
      </div>
      <div className='z-20'>
        <DeviceInfo expand onlyShowDetail openState={detailExpand} onDetailExpand={setDetailExpand} />
      </div>
    </>
  )
}

export default DeviceSidebarDropdown