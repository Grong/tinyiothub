'use client'

import { useTranslation } from 'react-i18next'
import { usePathname } from 'next/navigation'
import Nav from '../nav'
import { 
  RiDeviceLine,
  RiDeviceFill,
} from '@remixicon/react'

interface DeviceNavProps {
  className?: string
}

const DeviceNav = ({ className }: DeviceNavProps) => {
  const { t } = useTranslation('common')
  const pathname = usePathname()

  return (
    <Nav
      className={className}
      icon={<RiDeviceLine className='h-4 w-4' />}
      activeIcon={<RiDeviceFill className='h-4 w-4' />}
      text={t('navigation.devices')}
      activeSegment={['devices']}
      link='/devices'
      isActive={pathname === '/devices'}
    />
  )
}

export default DeviceNav