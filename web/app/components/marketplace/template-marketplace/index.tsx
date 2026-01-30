'use client'

import React from 'react'
import { TemplateMarketplaceProvider } from './context'
import Description from './description'
import SearchBox from './search-box'
import CategoryFilter from './category-filter'
import TemplateListWrapper from './template-list-wrapper'

const TemplateMarketplace: React.FC = () => {
  return (
    <TemplateMarketplaceProvider>
      <Description />
      <div className="sticky top-[60px] z-10 mt-4 bg-background-body">
        <SearchBox />
        <CategoryFilter />
      </div>
      <TemplateListWrapper />
    </TemplateMarketplaceProvider>
  )
}

export default TemplateMarketplace
