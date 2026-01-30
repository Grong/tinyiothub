'use client'

import React from 'react'
import { useTemplates } from '@/service/templates'
import Loading from '@/app/components/base/loading'
import TemplateCard from './template-card'

const InstalledTemplates: React.FC = () => {
  const { data: templates, isLoading } = useTemplates()

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Loading />
      </div>
    )
  }

  if (!templates || templates.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <div className="text-6xl">📦</div>
        <div className="mt-4 text-lg font-medium text-text-secondary">
          暂无已安装模板
        </div>
        <div className="mt-2 text-sm text-text-tertiary">
          前往模板市场安装模板
        </div>
      </div>
    )
  }

  return (
    <div className="px-12 py-6">
      <div className="mb-4">
        <div className="title-xl-semi-bold text-text-primary">
          已安装 {templates.length} 个模板
        </div>
      </div>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {templates.map((template) => (
          <TemplateCard
            key={template.id}
            template={template}
          />
        ))}
      </div>
    </div>
  )
}

export default InstalledTemplates
