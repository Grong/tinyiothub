'use client'

import { useTranslation } from 'react-i18next'
import { usePathname } from 'next/navigation'
import Nav from '../nav'
import { 
  RiPriceTagLine,
  RiPriceTagFill,
} from '@remixicon/react'

const TagNav = () => {
  const { t } = useTranslation('common')
  const pathname = usePathname()

  return (
    <Nav
      icon={<RiPriceTagLine className='h-4 w-4' />}
      activeIcon={<RiPriceTagFill className='h-4 w-4' />}
      text={t('navigation.tags')}
      activeSegment={['tags']}
      link='/tags'
      isActive={pathname === '/tags'}
    />
  )
}

export default TagNav