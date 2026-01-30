'use client'

import React from 'react'
import Link from 'next/link'
import classNames from '@/utils/classnames'

type INavProps = {
  icon: React.ReactNode
  activeIcon?: React.ReactNode
  text: string
  activeSegment: string | string[]
  link: string
  isActive?: boolean
  className?: string
}

const Nav = ({
  icon,
  activeIcon,
  text,
  activeSegment,
  link,
  isActive = false,
  className,
}: INavProps) => {
  return (
    <div className={classNames(
      'flex h-8 max-w-[670px] shrink-0 items-center rounded-xl px-0.5 text-sm font-medium max-[1024px]:max-w-[400px]',
      isActive && 'bg-components-main-nav-nav-button-bg-active font-semibold shadow-md',
      !isActive && 'hover:bg-components-main-nav-nav-button-bg-hover',
      className
    )}>
      <Link href={link}>
        <div
          className={classNames(
            'flex h-7 cursor-pointer items-center rounded-[10px] px-2.5',
            isActive ? 'text-components-main-nav-nav-button-text-active' : 'text-components-main-nav-nav-button-text',
          )}
        >
          <div>
            {isActive && activeIcon ? activeIcon : icon}
          </div>
          <div className='ml-2 max-[1024px]:hidden'>
            {text}
          </div>
        </div>
      </Link>
    </div>
  )
}

export default Nav