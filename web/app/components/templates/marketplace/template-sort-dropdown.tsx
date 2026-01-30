'use client'

import React, { useState } from 'react'
import { RiArrowDownSLine, RiCheckLine } from '@remixicon/react'
import {
  PortalToFollowElem,
  PortalToFollowElemContent,
  PortalToFollowElemTrigger,
} from '@/app/components/base/portal-to-follow-elem'
import { cn } from '@/utils/classnames'
import { useTemplateMarketplaceContext, type SortOption } from './context'

const TemplateSortDropdown: React.FC = () => {
  const { sortBy, setSortBy } = useTemplateMarketplaceContext()
  const [open, setOpen] = useState(false)

  const sortOptions = [
    { value: 'name' as SortOption, label: '按名称排序' },
    { value: 'category' as SortOption, label: '按分类排序' },
    { value: 'manufacturer' as SortOption, label: '按厂商排序' },
    { value: 'created_at' as SortOption, label: '按创建时间排序' },
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

export default TemplateSortDropdown