'use client'

import React from 'react'
import Loading from '@/app/components/base/loading'
import { useTemplateMarketplaceContext } from './context'
import TemplateSortDropdown from './template-sort-dropdown'
import TemplateList from './template-list'

const TemplateListWrapper: React.FC = () => {
  const { 
    templates, 
    totalCount, 
    isLoading, 
    error,
    hasMore,
    loadMore 
  } = useTemplateMarketplaceContext()

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <div className="text-text-destructive">加载模板失败</div>
        <div className="mt-2 text-sm text-text-tertiary">{error.message}</div>
      </div>
    )
  }

  return (
    <div
      style={{ scrollbarGutter: 'stable' }}
      className="relative flex grow flex-col bg-background-default-subtle px-12 py-2"
    >
      {/* 结果统计和排序 */}
      <div className="mb-4 flex items-center pt-3">
        <div className="title-xl-semi-bold text-text-primary">
          找到 {totalCount} 个模板
        </div>
        <div className="mx-3 h-3.5 w-[1px] bg-divider-regular"></div>
        <TemplateSortDropdown />
      </div>

      {/* 加载状态 */}
      {isLoading && templates.length === 0 && (
        <div className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2">
          <Loading />
        </div>
      )}

      {/* 模板列表 */}
      {!isLoading || templates.length > 0 ? (
        <TemplateList 
          templates={templates}
          hasMore={hasMore}
          onLoadMore={loadMore}
          isLoading={isLoading}
        />
      ) : null}
    </div>
  )
}

export default TemplateListWrapper