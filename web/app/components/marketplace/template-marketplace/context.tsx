'use client'

import React, { createContext, useContext, useState, useCallback, useMemo } from 'react'
import { useMarketplaceTemplates, type TemplateMetadata } from '@/service/marketplace'

export type TemplateCategory = 'all' | 'sensor' | 'controller' | 'camera' | 'gateway' | 'others'

export type SortOption = 'name' | 'downloads' | 'rating' | 'updated_at'

interface TemplateMarketplaceContextType {
  searchText: string
  setSearchText: (text: string) => void
  selectedCategory: TemplateCategory
  setSelectedCategory: (category: TemplateCategory) => void
  sortBy: SortOption
  setSortBy: (sort: SortOption) => void
  
  templates: TemplateMetadata[]
  filteredTemplates: TemplateMetadata[]
  isLoading: boolean
  error: Error | null
  
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
  const [sortBy, setSortBy] = useState<SortOption>('downloads')
  const [page, setPage] = useState(1)
  const pageSize = 20

  const { data: templates = [], isLoading, error } = useMarketplaceTemplates()

  const filteredTemplates = useMemo(() => {
    let filtered = templates

    if (searchText.trim()) {
      const searchLower = searchText.toLowerCase()
      filtered = filtered.filter(template => 
        template.name.toLowerCase().includes(searchLower) ||
        template.description?.toLowerCase().includes(searchLower) ||
        template.manufacturer?.toLowerCase().includes(searchLower) ||
        template.tags?.some(tag => tag.toLowerCase().includes(searchLower))
      )
    }

    if (selectedCategory !== 'all') {
      filtered = filtered.filter(template => template.category === selectedCategory)
    }

    filtered.sort((a, b) => {
      switch (sortBy) {
        case 'name':
          return a.name.localeCompare(b.name)
        case 'downloads':
          return (b.downloads || 0) - (a.downloads || 0)
        case 'rating':
          return (b.rating || 0) - (a.rating || 0)
        case 'updated_at':
          return new Date(b.updatedAt || 0).getTime() - new Date(a.updatedAt || 0).getTime()
        default:
          return 0
      }
    })

    return filtered
  }, [templates, searchText, selectedCategory, sortBy])

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
