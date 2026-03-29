'use client'

import React from 'react'

export type TabType = 'templates' | 'drivers'

interface MarketplaceTabsProps {
  activeTab: TabType
  onTabChange: (tab: TabType) => void
}

export default function MarketplaceTabs({ activeTab, onTabChange }: MarketplaceTabsProps) {
  const tabs: { key: TabType; label: string }[] = [
    { key: 'templates', label: '设备模板' },
    { key: 'drivers', label: '驱动程序' },
  ]

  return (
    <div className="flex items-center gap-2 p-1.5 rounded-2xl glass-card w-fit">
      {tabs.map((tab) => (
        <button
          key={tab.key}
          onClick={() => onTabChange(tab.key)}
          className={`tab-button ${activeTab === tab.key ? 'active' : ''}`}
        >
          {tab.label}
        </button>
      ))}
    </div>
  )
}