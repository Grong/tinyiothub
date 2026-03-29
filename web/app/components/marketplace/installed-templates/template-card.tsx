'use client'

import React from 'react'
import { RiDeleteBinLine, RiEyeLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import { type ProcessedDeviceTemplate } from '@/service/templates'
import { useLocalizedText } from '@/utils/i18n-template'
import { cn } from '@/utils/classnames'

interface TemplateCardProps {
  template: ProcessedDeviceTemplate
}

const getCategoryIcon = (category: string) => {
  const icons: Record<string, string> = {
    sensors: '🌡️',
    controllers: '🎛️',
    cameras: '📷',
    gateways: '🌐',
    others: '📦',
  }
  return icons[category] || icons.others
}

const CornerMark = ({ text }: { text: string }) => {
  return (
    <div className="absolute right-0 top-0 flex pl-[13px]">
      <div className="h-0 w-0 border-b-[20px] border-r-[20px] border-b-transparent border-r-background-section"></div>
      <div className="system-2xs-medium-uppercase h-5 rounded-tr-xl bg-background-section pr-2 leading-5 text-text-tertiary">{text}</div>
    </div>
  )
}

const TemplateIcon = ({ category }: { category: string }) => {
  return (
    <div className="relative flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-components-button-secondary-bg text-2xl">
      {getCategoryIcon(category)}
    </div>
  )
}

const TemplateTitle = ({ title }: { title: string }) => {
  return (
    <div className="system-md-semibold truncate text-text-secondary">
      {title}
    </div>
  )
}

const TemplateOrgInfo = ({ 
  manufacturer, 
  category, 
  version,
  className 
}: { 
  manufacturer?: string
  category: string
  version?: string
  className?: string
}) => {
  return (
    <div className={cn('flex h-4 items-center space-x-0.5', className)}>
      <span className="system-xs-regular shrink-0 text-text-tertiary">{category}</span>
      {manufacturer && (
        <>
          <span className="system-xs-regular shrink-0 text-text-quaternary">/</span>
          <span className="system-xs-regular w-0 shrink-0 grow truncate text-text-tertiary">
            {manufacturer}
          </span>
        </>
      )}
      {version && (
        <>
          <span className="system-xs-regular shrink-0 text-text-quaternary">v</span>
          <span className="system-xs-regular shrink-0 text-text-tertiary">{version}</span>
        </>
      )}
    </div>
  )
}

const TemplateDescription = ({ 
  text, 
  className 
}: { 
  text: string
  className?: string 
}) => {
  return (
    <div className={cn('system-xs-regular h-8 line-clamp-2 text-text-tertiary', className)}>
      {text}
    </div>
  )
}

const TemplateCard: React.FC<TemplateCardProps> = ({
  template,
}) => {
  const getLocalizedText = useLocalizedText()
  
  const displayName = getLocalizedText(template.displayName || {}, template.name)
  const description = getLocalizedText(template.description || {}, '')

  const wrapClassName = cn(
    'group hover-bg-components-panel-on-panel-item-bg relative overflow-hidden rounded-2xl border border-white/40 glass-shadow glass bg-white/60 backdrop-blur-xl transition-all duration-300 hover:shadow-2xl hover:bg-white/80 hover:-translate-y-1'
  )

  return (
    <div className={wrapClassName}>
      <div className="p-4 pb-3">
        {template.isBuiltin && <CornerMark text="内置" />}
        
        <div className="flex">
          <TemplateIcon category={template.category} />
          <div className="ml-3 w-0 grow">
            <div className="flex h-5 items-center">
              <TemplateTitle title={displayName} />
            </div>
            <TemplateOrgInfo
              manufacturer={template.manufacturer}
              category={template.category}
              version={template.version}
              className="mt-0.5"
            />
          </div>
        </div>
        
        {description && (
          <TemplateDescription
            className="mt-3"
            text={description}
          />
        )}

        <div className="mt-3 flex items-center gap-4 text-xs text-text-tertiary">
          <div className="flex items-center gap-1">
            <span>属性:</span>
            <span className="font-medium">{template.properties?.length || 0}</span>
          </div>
          <div className="flex items-center gap-1">
            <span>命令:</span>
            <span className="font-medium">{template.commands?.length || 0}</span>
          </div>
        </div>
      </div>

      <div className="absolute bottom-0 left-0 z-10 hidden w-full items-center gap-x-2 bg-pipeline-template-card-hover-bg p-4 pt-8 group-hover:flex">
        <Button
          variant="ghost"
          size="small"
          className="flex-1"
        >
          <RiEyeLine className="mr-1 h-4 w-4" />
          查看
        </Button>
        <Button
          variant="ghost"
          size="small"
          className="flex-1 text-text-destructive hover:text-text-destructive"
        >
          <RiDeleteBinLine className="mr-1 h-4 w-4" />
          删除
        </Button>
      </div>
    </div>
  )
}

export default TemplateCard
