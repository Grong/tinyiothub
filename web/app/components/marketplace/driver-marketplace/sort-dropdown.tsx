'use client'

import React, { useState } from 'react'
import { RiArrowDownSLine, RiCheckLine } from '@remixicon/react'
import {
  PortalToFollowElem,
  PortalToFollowElemContent,
  PortalToFollowElemTrigger,
} from '@/app/components/base/portal-to-follow-elem'
import { cn } from '@/utils/classnames'
import { useDriverMarketplaceContext, type SortOption } from './context'

const SortDropdown: React.FC = () => {
  const { sortBy, setSortBy } = useDriverMarketplaceContext()
  const [open, setOpen] = useState(false)

  const sortOptions = [
    { value: 'downloads' as SortOption, label: '按下载量排序' },
    { value: 'rating' as SortOption, label: '按评分排序' },
    { value: 'name' as SortOption, label: '按名称排序' },
    { value: 'updated_at' as SortOption, label: '按更新时间排序' },
  ]

  const selectedOption = sortOptions.find(option => option.value === sortBy)

  return (
    <PortalToFollowElem
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      offset={4}
    >
      <PortalToFollowElemTrigger onClick={() => setOpen(!open)} asChild>
        <div className={cn(
          'system-sm-regular group flex h-8 cursor-pointer items-center rounded-lg bg-components-input-bg-normal px-3 text-components-input-text-filled hover:bg-state-base-hover-alt',
          open && 'bg-state-base-hover-alt'
        )}>
          <span className="mr-2">{selectedOption?.label}</span>
          <RiArrowDownSLine className={cn(
            'h-4 w-4 text-text-quaternary group-hover:text-text-secondary',
            open && 'text-text-secondary'
          )} />
        </div>
      </PortalToFollowElemTrigger>
      <PortalToFollowElemContent className="z-[9999]">
        <div className="min-w-[160px] rounded-xl border-[0.5px] border-components-panel-border bg-components-panel-bg-blur p-1 shadow-lg">
          {sortOptions.map((option) => (
            <div
              key={option.value}
              className="system-sm-medium flex h-8 cursor-pointer items-center rounded-lg px-2 text-text-secondary hover:bg-state-base-hover"
              onClick={() => {
                setSortBy(option.value)
                setOpen(false)
              }}
            >
              <div className="mr-1 grow px-1">
                {option.label}
              </div>
              {sortBy === option.value && <RiCheckLine className="h-4 w-4 shrink-0 text-text-accent" />}
            </div>
          ))}
        </div>
      </PortalToFollowElemContent>
    </PortalToFollowElem>
  )
}

export default SortDropdown
