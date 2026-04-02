'use client'

import React, { useState, useEffect } from 'react'
import { basePath } from '@/utils/var'
import MarketplaceSearch from './components/marketplace-search'
import { TabType } from './components/marketplace-tabs'
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
  const [isNavVisible, setIsNavVisible] = useState(true)

  const { data: templates = [], isLoading: templatesLoading } = useMarketplaceTemplates()
  const { data: drivers = [], isLoading: driversLoading } = useMarketplaceDrivers()

  useEffect(() => {
    document.title = 'TinyIoTHub | 智能物联网平台'
  }, [])

  // 滚动隐藏/显示导航
  useEffect(() => {
    let lastScrollY = 0

    const handleScroll = () => {
      const currentScrollY = window.scrollY
      const threshold = 80

      if (currentScrollY > lastScrollY && currentScrollY > threshold) {
        setIsNavVisible(false)
      } else {
        setIsNavVisible(true)
      }

      lastScrollY = currentScrollY
    }

    window.addEventListener('scroll', handleScroll, { passive: true })
    return () => window.removeEventListener('scroll', handleScroll)
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
      <nav className={`fixed inset-x-0 top-0 z-50 glass-nav border-b border-white/30 transition-transform duration-300 ease-out ${isNavVisible ? 'translate-y-0' : '-translate-y-full'}`}>
        <div className="px-4 md:px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-8">
              <a href="/" className="flex items-center gap-2 group">
                <img src={`${basePath}/logo.svg`} alt="logo" className="h-9 w-9 object-contain homepage-logo" />
                <span className="text-xl font-bold text-primary homepage-nav-text">TinyIoTHub</span>
              </a>
              <div className="hidden lg:flex items-center gap-1">
                <a href="/" className="flex h-8 items-center rounded-xl px-3 text-sm font-medium text-components-main-nav-nav-button-text hover:bg-components-main-nav-nav-button-bg-hover transition-colors homepage-nav-text">首页</a>
                <a href="/marketplace" className="flex h-8 items-center rounded-xl px-3 text-sm font-medium bg-components-main-nav-nav-button-bg-active text-components-main-nav-nav-button-text-active font-semibold shadow-md transition-colors">市场</a>
                <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" className="flex h-8 items-center rounded-xl px-3 text-sm font-medium text-components-main-nav-nav-button-text hover:bg-components-main-nav-nav-button-bg-hover transition-colors homepage-nav-text">文档</a>
              </div>
            </div>
            <div className="flex items-center gap-4">
              <a href="/signin" className="text-sm font-medium text-secondary hover:text-primary transition-colors homepage-nav-text">登录</a>
              <a href="/signin" className="rounded-lg bg-components-button-primary-bg text-components-button-primary-text px-5 py-2.5 text-sm font-semibold hover:bg-components-button-primary-bg-hover transition-all">免费试用</a>
            </div>
          </div>
        </div>
      </nav>

      {/* Main Content */}
      <div className="px-4 md:px-6 lg:px-8 pt-20 py-8">
        {/* Hero Section */}
        <div className="text-center mb-12 pt-4">
          <h1 className="text-3xl md:text-4xl font-bold text-gray-900 mb-4 marketplace-hero-title">
            设备市场
          </h1>
          <p className="text-base text-gray-500 max-w-xl mx-auto leading-relaxed marketplace-hero-subtitle">
            探索来自社区的优质设备模板与驱动，开箱即用，快速接入传感器、执行器与工业设备
          </p>
        </div>

        {/* Search + Segmented Control */}
        <div className="mb-8">
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
            onTabChange={setActiveTab}
            activeTab={activeTab}
          />
        </div>

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
