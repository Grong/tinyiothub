'use client'

import React, { createContext, useContext, useState, useCallback, useMemo } from 'react'
import { useTemplates, type ProcessedDeviceTemplate } from '@/service/templates'

export type TemplateCategory = 'all' | 'sensors' | 'controllers' | 'cameras' | 'gateways' | 'others'

export type SortOption = 'name' | 'category' | 'manufacturer' | 'created_at'

interface TemplateMarketplaceContextType {
  // 搜索和过滤
  searchText: string
  setSearchText: (text: string) => void
  selectedCategory: TemplateCategory
  setSelectedCategory: (category: TemplateCategory) => void
  sortBy: SortOption
  setSortBy: (sort: SortOption) => void
  
  // 数据
  templates: ProcessedDeviceTemplate[]
  filteredTemplates: ProcessedDeviceTemplate[]
  isLoading: boolean
  error: Error | null
  
  // 分页
  page: number
  pageSize: number
  totalCount: number
  hasMore: boolean
  loadMore: () => void
}

const TemplateMarketplaceContext = createContext<TemplateMarketplaceContextType | null>(null)

export const useTemplateMarketplaceContext = () => {
  const context = useContext(TemplateMarketplaceContext)
  if (!context) {
    throw new Error('useTemplateMarketplaceContext must be used within TemplateMarketplaceProvider')
  }
  return context
}

interface TemplateMarketplaceProviderProps {
  children: React.ReactNode
}

export const TemplateMarketplaceProvider: React.FC<TemplateMarketplaceProviderProps> = ({ children }) => {
  const [searchText, setSearchText] = useState('')
  const [selectedCategory, setSelectedCategory] = useState<TemplateCategory>('all')
  const [sortBy, setSortBy] = useState<SortOption>('name')
  const [page, setPage] = useState(1)
  const pageSize = 20

  // 获取模板数据
  const { data: templates = [], isLoading, error } = useTemplates()

  // 过滤和排序模板
  const filteredTemplates = useMemo(() => {
    let filtered = templates

    // 按搜索文本过滤
    if (searchText.trim()) {
      const searchLower = searchText.toLowerCase()
      filtered = filtered.filter(template => 
        template.name.toLowerCase().includes(searchLower) ||
        template.displayName?.zh?.toLowerCase().includes(searchLower) ||
        template.displayName?.en?.toLowerCase().includes(searchLower) ||
        template.manufacturer?.toLowerCase().includes(searchLower) ||
        template.description?.zh?.toLowerCase().includes(searchLower) ||
        template.description?.en?.toLowerCase().includes(searchLower)
      )
    }

    // 按分类过滤
    if (selectedCategory !== 'all') {
      filtered = filtered.filter(template => template.category === selectedCategory)
    }

    // 排序
    filtered.sort((a, b) => {
      switch (sortBy) {
        case 'name':
          return a.name.localeCompare(b.name)
        case 'category':
          return a.category.localeCompare(b.category)
        case 'manufacturer':
          return (a.manufacturer || '').localeCompare(b.manufacturer || '')
        case 'created_at':
          return new Date(b.createdAt || 0).getTime() - new Date(a.createdAt || 0).getTime()
        default:
          return 0
      }
    })

    return filtered
  }, [templates, searchText, selectedCategory, sortBy])

  // 分页数据
  const paginatedTemplates = useMemo(() => {
    return filteredTemplates.slice(0, page * pageSize)
  }, [filteredTemplates, page, pageSize])

  const totalCount = filteredTemplates.length
  const hasMore = paginatedTemplates.length < totalCount

  const loadMore = useCallback(() => {
    if (hasMore) {
      setPage(prev => prev + 1)
    }
  }, [hasMore])

  // 重置分页当搜索条件改变时
  React.useEffect(() => {
    setPage(1)
  }, [searchText, selectedCategory, sortBy])

  const contextValue: TemplateMarketplaceContextType = {
    searchText,
    setSearchText,
    selectedCategory,
    setSelectedCategory,
    sortBy,
    setSortBy,
    templates: paginatedTemplates,
    filteredTemplates,
    isLoading,
    error,
    page,
    pageSize,
    totalCount,
    hasMore,
    loadMore,
  }

  return (
    <TemplateMarketplaceContext.Provider value={contextValue}>
      {children}
    </TemplateMarketplaceContext.Provider>
  )
}