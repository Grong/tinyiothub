'use client'

import React, { createContext, useContext, useState, useCallback, useMemo } from 'react'
import { useMarketplaceDrivers, type DriverMetadata } from '@/service/marketplace'

export type DriverProtocol = 'all' | 'modbus' | 'onvif' | 'snmp' | 'mqtt' | 'bacnet' | 'opcua' | 'others'

export type SortOption = 'name' | 'downloads' | 'rating' | 'updated_at'

interface DriverMarketplaceContextType {
  searchText: string
  setSearchText: (text: string) => void
  selectedProtocol: DriverProtocol
  setSelectedProtocol: (protocol: DriverProtocol) => void
  sortBy: SortOption
  setSortBy: (sort: SortOption) => void
  
  drivers: DriverMetadata[]
  filteredDrivers: DriverMetadata[]
  isLoading: boolean
  error: Error | null
  
  page: number
  pageSize: number
  totalCount: number
  hasMore: boolean
  loadMore: () => void
}

const DriverMarketplaceContext = createContext<DriverMarketplaceContextType | null>(null)

export const useDriverMarketplaceContext = () => {
  const context = useContext(DriverMarketplaceContext)
  if (!context) {
    throw new Error('useDriverMarketplaceContext must be used within DriverMarketplaceProvider')
  }
  return context
}

interface DriverMarketplaceProviderProps {
  children: React.ReactNode
}

export const DriverMarketplaceProvider: React.FC<DriverMarketplaceProviderProps> = ({ children }) => {
  const [searchText, setSearchText] = useState('')
  const [selectedProtocol, setSelectedProtocol] = useState<DriverProtocol>('all')
  const [sortBy, setSortBy] = useState<SortOption>('downloads')
  const [page, setPage] = useState(1)
  const pageSize = 20

  const { data: drivers = [], isLoading, error } = useMarketplaceDrivers()

  const filteredDrivers = useMemo(() => {
    let filtered = drivers

    if (searchText.trim()) {
      const searchLower = searchText.toLowerCase()
      filtered = filtered.filter(driver => 
        driver.name.toLowerCase().includes(searchLower) ||
        driver.description?.toLowerCase().includes(searchLower) ||
        driver.protocol?.toLowerCase().includes(searchLower) ||
        driver.tags?.some(tag => tag.toLowerCase().includes(searchLower))
      )
    }

    if (selectedProtocol !== 'all') {
      filtered = filtered.filter(driver => driver.protocol.toLowerCase() === selectedProtocol)
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
  }, [drivers, searchText, selectedProtocol, sortBy])

  const paginatedDrivers = useMemo(() => {
    return filteredDrivers.slice(0, page * pageSize)
  }, [filteredDrivers, page, pageSize])

  const totalCount = filteredDrivers.length
  const hasMore = paginatedDrivers.length < totalCount

  const loadMore = useCallback(() => {
    if (hasMore) {
      setPage(prev => prev + 1)
    }
  }, [hasMore])

  React.useEffect(() => {
    setPage(1)
  }, [searchText, selectedProtocol, sortBy])

  const contextValue: DriverMarketplaceContextType = {
    searchText,
    setSearchText,
    selectedProtocol,
    setSelectedProtocol,
    sortBy,
    setSortBy,
    drivers: paginatedDrivers,
    filteredDrivers,
    isLoading,
    error,
    page,
    pageSize,
    totalCount,
    hasMore,
    loadMore,
  }

  return (
    <DriverMarketplaceContext.Provider value={contextValue}>
      {children}
    </DriverMarketplaceContext.Provider>
  )
}
