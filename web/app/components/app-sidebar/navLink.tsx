'use client'
import React, { useEffect, useState } from 'react'
import classNames from '@/utils/classnames'
import type { RemixiconComponentType } from '@remixicon/react'

export type NavIcon = React.ComponentType<
  React.PropsWithoutRef<React.ComponentProps<'svg'>> & {
    title?: string | undefined
    titleId?: string | undefined
  }> | RemixiconComponentType

export type NavLinkProps = {
  name: string
  href: string
  iconMap: {
    selected: NavIcon
    normal: NavIcon
  }
  mode?: string
  disabled?: boolean
}

const NavLink = ({
  name,
  href,
  iconMap,
  mode = 'expand',
  disabled = false,
}: NavLinkProps) => {
  const [isActive, setIsActive] = useState(false)

  useEffect(() => {
    // 检查当前 hash 是否匹配
    const checkActive = () => {
      const currentHash = window.location.hash
      setIsActive(currentHash === href)
    }

    checkActive()
    
    // 监听 hash 变化
    window.addEventListener('hashchange', checkActive)
    return () => window.removeEventListener('hashchange', checkActive)
  }, [href])

  const NavIcon = isActive ? iconMap.selected : iconMap.normal

  const renderIcon = () => (
    <div className={classNames(mode !== 'expand' && '-ml-1')}>
      <NavIcon className="h-4 w-4 shrink-0" aria-hidden="true" />
    </div>
  )

  if (disabled) {
    return (
      <button
        key={name}
        type='button'
        disabled
        className={classNames(
          'system-sm-medium flex h-8 cursor-not-allowed items-center rounded-lg text-components-menu-item-text opacity-30 hover:bg-components-menu-item-bg-hover',
          'pl-3 pr-1',
        )}
        title={mode === 'collapse' ? name : ''}
        aria-disabled
      >
        {renderIcon()}
        <span
          className={classNames(
            'overflow-hidden whitespace-nowrap transition-all duration-200 ease-in-out',
            mode === 'expand'
              ? 'ml-2 max-w-none opacity-100'
              : 'ml-0 max-w-0 opacity-0',
          )}
        >
          {name}
        </span>
      </button>
    )
  }

  return (
    <a
      key={name}
      href={href}
      className={classNames(
        isActive
          ? 'system-sm-semibold border-b-[0.25px] border-l-[0.75px] border-r-[0.25px] border-t-[0.75px] border-effects-highlight-lightmode-off bg-components-menu-item-bg-active text-text-accent-light-mode-only'
          : 'system-sm-medium text-components-menu-item-text hover:bg-components-menu-item-bg-hover hover:text-components-menu-item-text-hover',
        'flex h-8 items-center rounded-lg pl-3 pr-1',
      )}
      title={mode === 'collapse' ? name : ''}
    >
      {renderIcon()}
      <span
        className={classNames(
          'overflow-hidden whitespace-nowrap transition-all duration-200 ease-in-out',
          mode === 'expand'
            ? 'ml-2 max-w-none opacity-100'
            : 'ml-0 max-w-0 opacity-0',
        )}
      >
        {name}
      </span>
    </a>
  )
}

export default React.memo(NavLink)