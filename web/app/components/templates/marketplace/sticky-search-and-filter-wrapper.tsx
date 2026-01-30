'use client'

import React from 'react'
import { cn } from '@/utils/classnames'
import TemplateSearchBox from './template-search-box'
import TemplateCategoryFilter from './template-category-filter'

const StickySearchAndFilterWrapper: React.FC = () => {
  return (
    <div className="sticky top-[60px] z-10 mt-4 bg-background-body">
      <TemplateSearchBox />
      <TemplateCategoryFilter />
    </div>
  )
}

export default StickySearchAndFilterWrapper