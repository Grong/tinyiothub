'use client'

import { useTranslation } from 'react-i18next'
import { usePathname } from 'next/navigation'
import Nav from '../nav'
import { 
  RiDashboardLine,
  RiDashboardFill,
} from '@remixicon/react'

const AppNav = () => {
  const { t } = useTranslation('common')
  const pathname = usePathname()

  return (
    <Nav
      icon={<RiDashboardLine className='h-4 w-4' />}
      activeIcon={<RiDashboardFill className='h-4 w-4' />}
      text={t('nav.dashboard')}
      activeSegment={['dashboard']}
      link='/dashboard'
      isActive={pathname === '/dashboard'}
    />
  )
}

export default AppNav