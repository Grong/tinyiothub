'use client'

import React from 'react'
import { MagnifyingGlassIcon } from '@heroicons/react/24/outline'
import { TabType } from './marketplace-tabs'

interface MarketplaceSearchProps {
  searchQuery: string
  onSearchChange: (query: string) => void
  filterOptions: { value: string; label: string }[]
  sortOptions: { value: string; label: string }[]
  activeFilter: string
  activeSort: string
  onFilterChange: (filter: string) => void
  onSortChange: (sort: string) => void
  tabType?: 'templates' | 'drivers'
  onTabChange: (tab: TabType) => void
  activeTab: TabType
}

export default function MarketplaceSearch({
  searchQuery,
  onSearchChange,
  filterOptions,
  sortOptions,
  activeFilter,
  activeSort,
  onFilterChange,
  onSortChange,
  tabType,
  onTabChange,
  activeTab,
}: MarketplaceSearchProps) {
  return (
    <div className="flex flex-col lg:flex-row gap-3 items-start lg:items-center">
      {/* Segmented Control */}
      <div className="flex items-center shrink-0 rounded-xl bg-components-input-bg-normal p-1">
        <button
          onClick={() => onTabChange('templates')}
          className={`px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 whitespace-nowrap ${
            activeTab === 'templates'
              ? 'bg-components-button-primary-bg text-components-button-primary-text'
              : 'text-secondary hover:bg-state-base-hover hover:text-primary'
          }`}
        >
          设备模板
        </button>
        <button
          onClick={() => onTabChange('drivers')}
          className={`px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 whitespace-nowrap ${
            activeTab === 'drivers'
              ? 'bg-components-button-primary-bg text-components-button-primary-text'
              : 'text-secondary hover:bg-state-base-hover hover:text-primary'
          }`}
        >
          驱动程序
        </button>
      </div>

      {/* Search + controls row */}
      <div className="flex flex-col sm:flex-row gap-3 w-full lg:w-auto lg:flex-1 min-w-0">
        {/* Search input */}
        <div className="relative flex-1 min-w-0">
          <MagnifyingGlassIcon className="absolute left-3.5 top-1/2 -translate-y-1/2 h-4 w-4 text-components-input-text-placeholder" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={tabType === 'drivers' ? '搜索驱动...' : '搜索模板...'}
            className="w-full pl-10 pr-4 py-2.5 bg-components-input-bg-normal text-components-input-text-filled placeholder:text-components-input-text-placeholder border border-transparent rounded-xl text-sm focus:outline-none focus:border-components-input-border-active focus:bg-components-input-bg-active transition-all"
          />
        </div>

        {/* Filter & Sort */}
        <div className="flex flex-row gap-2 flex-1 min-w-0">
          <select
            value={activeFilter}
            onChange={(e) => onFilterChange(e.target.value)}
            className="px-3 py-2 flex-1 min-w-[120px] bg-components-input-bg-normal text-components-input-text-filled border border-transparent rounded-xl text-sm focus:outline-none focus:border-components-input-border-active focus:bg-components-input-bg-active cursor-pointer"
          >
            {filterOptions.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>

          <select
            value={activeSort}
            onChange={(e) => onSortChange(e.target.value)}
            className="px-3 py-2 flex-1 min-w-[120px] bg-components-input-bg-normal text-components-input-text-filled border border-transparent rounded-xl text-sm focus:outline-none focus:border-components-input-border-active focus:bg-components-input-bg-active cursor-pointer"
          >
            {sortOptions.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      </div>
    </div>
  )
}
