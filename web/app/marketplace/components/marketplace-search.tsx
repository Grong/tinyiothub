'use client'

import React from 'react'
import { MagnifyingGlassIcon } from '@heroicons/react/24/outline'

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
}: MarketplaceSearchProps) {
  return (
    <div className="glass-card p-4 mb-6">
      <div className="flex flex-col lg:flex-row gap-4">
        {/* 搜索框 */}
        <div className="relative flex-1">
          <MagnifyingGlassIcon className="absolute left-3 top-1/2 -translate-y-1/2 h-5 w-5 text-gray-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={tabType === 'drivers' ? '搜索驱动...' : '搜索模板...'}
            className="glass-search pl-10"
          />
        </div>

        {/* 筛选和排序 */}
        <div className="flex gap-3">
          <select
            value={activeFilter}
            onChange={(e) => onFilterChange(e.target.value)}
            className="glass-search w-auto min-w-[120px]"
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
            className="glass-search w-auto min-w-[120px]"
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
