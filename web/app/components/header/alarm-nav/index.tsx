'use client'
import { usePathname } from 'next/navigation'
import { useTranslation } from 'react-i18next'
import Nav from '../nav'
import { 
  RiAlarmWarningLine,
  RiAlarmWarningFill,
} from '@remixicon/react'

const AlarmNav = () => {
  const pathname = usePathname()
  const { t } = useTranslation('common')

  return (
    <Nav
      icon={<RiAlarmWarningLine className='h-4 w-4' />}
      activeIcon={<RiAlarmWarningFill className='h-4 w-4' />}
      text={t('navigation.alarms')}
      activeSegment={['alarms']}
      link='/alarms'
      isActive={pathname === '/alarms'}
    />
  )
}

export default AlarmNav
