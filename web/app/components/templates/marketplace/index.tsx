'use client'

import React from 'react'
import { TemplateMarketplaceProvider } from './context'
import Description from './description'
import StickySearchAndFilterWrapper from './sticky-search-and-filter-wrapper'
import TemplateListWrapper from './template-list-wrapper'

const TemplateMarketplace: React.FC = () => {
  return (
    <TemplateMarketplaceProvider>
      <Description />
      <StickySearchAndFilterWrapper />
      <TemplateListWrapper />
    </TemplateMarketplaceProvider>
  )
}

export default TemplateMarketplace