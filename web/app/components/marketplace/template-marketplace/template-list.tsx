'use client'

import React from 'react'
import Button from '@/app/components/base/button'
import Loading from '@/app/components/base/loading'
import { type TemplateMetadata } from '@/service/marketplace'
import TemplateCard from './template-card'

interface TemplateListProps {
  templates: TemplateMetadata[]
  hasMore: boolean
  onLoadMore: () => void
  isLoading: boolean
}

const TemplateList: React.FC<TemplateListProps> = ({
  templates,
  hasMore,
  onLoadMore,
  isLoading,
}) => {
  if (templates.length === 0 && !isLoading) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <div className="text-6xl">📦</div>
        <div className="mt-4 text-lg font-medium text-text-secondary">
          没有找到匹配的模板
        </div>
        <div className="mt-2 text-sm text-text-tertiary">
          尝试调整搜索条件或浏览其他分类
        </div>
      </div>
    )
  }

  return (
    <div className="pb-8">
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {templates.map((template) => (
          <TemplateCard
            key={template.id}
            template={template}
          />
        ))}
      </div>

      {hasMore && (
        <div className="mt-8 flex justify-center">
          <Button
            variant="secondary"
            onClick={onLoadMore}
            disabled={isLoading}
            className="min-w-[120px]"
          >
            {isLoading ? (
              <>
                <Loading />
                加载中...
              </>
            ) : (
              '加载更多'
            )}
          </Button>
        </div>
      )}
    </div>
  )
}

export default TemplateList
