'use client'

import Link from 'next/link'
import { useTranslation } from 'react-i18next'
import { useSelectedLayoutSegment } from 'next/navigation'
import {
  RiStore2Fill,
  RiStore2Line,
} from '@remixicon/react'
import classNames from '@/utils/classnames'

type MarketplaceNavProps = {
  className?: string
}

const MarketplaceNav = ({
  className,
}: MarketplaceNavProps) => {
  const { t } = useTranslation('common')
  const selectedSegment = useSelectedLayoutSegment()
  const activated = selectedSegment === 'marketplace'

  return (
    <Link href="/marketplace" className={classNames(
      className,
      'group text-sm font-medium',
      activated && 'hover:bg-components-main-nav-nav-button-bg-active-hover bg-components-main-nav-nav-button-bg-active font-semibold shadow-md',
      !activated && 'hover:bg-components-main-nav-nav-button-bg-hover text-components-main-nav-nav-button-text hover:text-components-main-nav-nav-button-text-hover',
    )}>
      {
        activated
          ? <RiStore2Fill className='h-4 w-4' />
          : <RiStore2Line className='h-4 w-4' />
      }
      <div className='ml-2 max-[1024px]:hidden'>
        市场
      </div>
    </Link>
  )
}

export default MarketplaceNav
