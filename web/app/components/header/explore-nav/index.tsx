'use client'

import { useTranslation } from 'react-i18next'
import Link from 'next/link'
import { useSelectedLayoutSegment } from 'next/navigation'
import {
  RiCompassFill,
  RiCompassLine,
} from '@remixicon/react'
import classNames from '@/utils/classnames'

type ExploreNavProps = {
  className?: string
}

const ExploreNav = ({
  className,
}: ExploreNavProps) => {
  const { t } = useTranslation('common')
  const selectedSegment = useSelectedLayoutSegment()
  const activated = selectedSegment === 'explore'

  return (
    <Link href="/explore" className={classNames(
      className, 'group',
      activated && 'bg-components-main-nav-nav-button-bg-active shadow-md',
      activated ? 'text-components-main-nav-nav-button-text-active' : 'text-components-main-nav-nav-button-text hover:bg-components-main-nav-nav-button-bg-hover',
    )}>
      {
        activated
          ? <RiCompassFill className='h-4 w-4' />
          : <RiCompassLine className='h-4 w-4' />
      }
      <div className='ml-2 max-[1024px]:hidden'>
        {t('menus.explore')}
      </div>
    </Link>
  )
}

export default ExploreNav