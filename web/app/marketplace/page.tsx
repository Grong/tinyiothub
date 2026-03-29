'use client'

import React, { useState, useEffect } from 'react'
import { ArrowRightIcon } from '@heroicons/react/24/outline'
import MarketplaceTabs, { TabType } from './components/marketplace-tabs'
import MarketplaceSearch from './components/marketplace-search'
import TemplateGrid from './components/template-grid'
import DriverGrid from './components/driver-grid'
import {
  useMarketplaceTemplates,
  useMarketplaceDrivers,
} from '@/service/marketplace'
import './styles/marketplace.css'

export default function MarketplacePage() {
  const [activeTab, setActiveTab] = useState<TabType>('templates')
  const [searchQuery, setSearchQuery] = useState('')
  const [activeFilter, setActiveFilter] = useState('all')
  const [activeSort, setActiveSort] = useState('popular')

  const { data: templates = [], isLoading: templatesLoading } = useMarketplaceTemplates()
  const { data: drivers = [], isLoading: driversLoading } = useMarketplaceDrivers()

  useEffect(() => {
    document.title = 'TinyIoTHub | 智能物联网平台'
  }, [])

  // 筛选和排序逻辑
  const filteredTemplates = templates.filter((t) =>
    t.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    t.description.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const filteredDrivers = drivers.filter((d) =>
    d.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    d.description.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const templateFilters = [
    { value: 'all', label: '全部分类' },
    { value: 'sensor', label: '传感器' },
    { value: 'actuator', label: '执行器' },
  ]

  const driverFilters = [
    { value: 'all', label: '全部协议' },
    { value: 'modbus', label: 'Modbus' },
    { value: 'onvif', label: 'ONVIF' },
    { value: 'snmp', label: 'SNMP' },
    { value: 'mqtt', label: 'MQTT' },
  ]

  const sortOptions = [
    { value: 'popular', label: '最受欢迎' },
    { value: 'recent', label: '最新' },
    { value: 'rating', label: '评分最高' },
  ]

  return (
    <div className="marketplace-bg">
      {/* Navigation */}
      <nav className="sticky top-0 z-50 glass-nav border-b border-white/30">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-8">
              <a href="/" className="flex items-center gap-2 group">
                <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-blue-600 to-blue-700 text-white shadow-lg shadow-blue-600/30">
                  <ArrowRightIcon className="h-5 w-5" />
                </div>
                <span className="text-xl font-bold text-gray-900">TinyIoTHub</span>
              </a>
              <div className="hidden lg:flex items-center gap-8">
                <a href="/marketplace" className="text-sm font-medium text-blue-600">市场</a>
                <a href="https://docs.tinyiothub.com" className="text-sm font-medium text-gray-600">文档</a>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <a href="/signin" className="text-sm font-medium text-gray-600">登录</a>
              <a href="/signin" className="rounded-lg bg-blue-600 px-5 py-2.5 text-sm font-semibold text-white">免费试用</a>
            </div>
          </div>
        </div>
      </nav>

      {/* Main Content */}
      <div className="px-6 lg:px-8 py-8 max-w-7xl mx-auto">
        {/* Tabs */}
        <div className="flex justify-center mb-8">
          <MarketplaceTabs activeTab={activeTab} onTabChange={setActiveTab} />
        </div>

        {/* Search */}
        <MarketplaceSearch
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          filterOptions={activeTab === 'templates' ? templateFilters : driverFilters}
          sortOptions={sortOptions}
          activeFilter={activeFilter}
          activeSort={activeSort}
          onFilterChange={setActiveFilter}
          onSortChange={setActiveSort}
          tabType={activeTab}
        />

        {/* Grid */}
        <div className="transition-all duration-300">
          {activeTab === 'templates' ? (
            <TemplateGrid templates={filteredTemplates} isLoading={templatesLoading} />
          ) : (
            <DriverGrid drivers={filteredDrivers} isLoading={driversLoading} />
          )}
        </div>
      </div>
    </div>
  )
}
