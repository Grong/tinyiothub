'use client'

import React from 'react'
import { RiSearchLine } from '@remixicon/react'
import Input from '@/app/components/base/input'
import { useTemplateMarketplaceContext } from './context'

const SearchBox: React.FC = () => {
  const { searchText, setSearchText } = useTemplateMarketplaceContext()

  return (
    <div className="mx-auto w-[640px] shrink-0 px-12">
      <div className="relative">
        <Input
          className="w-full pl-10"
          placeholder="搜索设备模板..."
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
        />
        <RiSearchLine className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-text-tertiary" />
      </div>
    </div>
  )
}

export default SearchBox
