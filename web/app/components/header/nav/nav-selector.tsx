'use client'

import React from 'react'

export type NavItem = {
  id: string
  name: string
  link: string
  icon?: string
  icon_type?: string
  icon_background?: string
  icon_url?: string
  mode?: string
}

export type INavSelectorProps = {
  curNav?: Omit<NavItem, 'link'>
  navigationItems?: NavItem[]
  createText?: string
  onCreate?: (state?: string) => void
  onLoadMore?: () => void
}

const NavSelector = ({
  isApp,
  curNav,
  navigationItems,
  createText,
  onCreate,
  onLoadMore,
}: INavSelectorProps & { isApp: boolean }) => {
  return (
    <div className="flex items-center">
      {curNav && (
        <div className="ml-2 text-sm font-medium text-components-main-nav-nav-button-text-active">
          {curNav.name}
        </div>
      )}
    </div>
  )
}

export default NavSelector