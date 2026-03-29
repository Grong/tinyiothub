'use client'

import React from 'react'
import TemplateCard from '@/app/components/marketplace/template-marketplace/template-card'
import type { TemplateMetadata } from '@/service/marketplace'

interface TemplateGridProps {
  templates: TemplateMetadata[]
  isLoading?: boolean
}

export default function TemplateGrid({ templates, isLoading }: TemplateGridProps) {
  if (isLoading) {
    return (
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
        {[...Array(8)].map((_, i) => (
          <div key={i} className="glass-card h-48 animate-pulse" />
        ))}
      </div>
    )
  }

  if (templates.length === 0) {
    return (
      <div className="glass-card p-12 text-center">
        <p className="text-gray-500">暂无模板</p>
      </div>
    )
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
      {templates.map((template) => (
        <TemplateCard key={template.id} template={template} />
      ))}
    </div>
  )
}
