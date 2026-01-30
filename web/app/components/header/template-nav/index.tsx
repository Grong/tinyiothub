'use client'

import { useTranslation } from 'react-i18next'
import { usePathname } from 'next/navigation'
import Nav from '../nav'
import { 
  RiFileTextLine,
  RiFileTextFill,
} from '@remixicon/react'

interface TemplateNavProps {
  className?: string
}

const TemplateNav = ({ className }: TemplateNavProps) => {
  const { t } = useTranslation('common')
  const pathname = usePathname()

  return (
    <Nav
      className={className}
      icon={<RiFileTextLine className='h-4 w-4' />}
      activeIcon={<RiFileTextFill className='h-4 w-4' />}
      text={t('navigation.templates')}
      activeSegment={['templates']}
      link='/templates'
      isActive={pathname === '/templates'}
    />
  )
}

export default TemplateNav