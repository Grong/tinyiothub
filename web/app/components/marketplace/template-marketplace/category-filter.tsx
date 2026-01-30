'use client'

import React from 'react'
import { 
  RiApps2Line,
  RiSensorLine,
  RiSettings3Line,
  RiCameraLine,
  RiWifiLine,
  RiMoreLine
} from '@remixicon/react'
import { cn } from '@/utils/classnames'
import { useTemplateMarketplaceContext, type TemplateCategory } from './context'

const CategoryFilter: React.FC = () => {
  const { selectedCategory, setSelectedCategory } = useTemplateMarketplaceContext()

  const categories = [
    {
      value: 'all' as TemplateCategory,
      text: '全部',
      icon: <RiApps2Line className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'sensor' as TemplateCategory,
      text: '传感器',
      icon: <RiSensorLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'controller' as TemplateCategory,
      text: '控制器',
      icon: <RiSettings3Line className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'camera' as TemplateCategory,
      text: '摄像头',
      icon: <RiCameraLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'gateway' as TemplateCategory,
      text: '网关',
      icon: <RiWifiLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'others' as TemplateCategory,
      text: '其他',
      icon: <RiMoreLine className="mr-1.5 h-4 w-4" />,
    },
  ]

  return (
    <div className="flex shrink-0 items-center justify-center space-x-2 bg-background-body py-3">
      {categories.map(category => (
        <div
          key={category.value}
          className={cn(
            'system-md-medium flex h-8 cursor-pointer items-center rounded-xl border border-transparent px-3 text-text-tertiary hover:bg-state-base-hover hover:text-text-secondary',
            selectedCategory === category.value && 'border-components-main-nav-nav-button-border !bg-components-main-nav-nav-button-bg-active !text-components-main-nav-nav-button-text-active shadow-xs',
          )}
          onClick={() => setSelectedCategory(category.value)}
        >
          {category.icon}
          {category.text}
        </div>
      ))}
    </div>
  )
}

export default CategoryFilter
