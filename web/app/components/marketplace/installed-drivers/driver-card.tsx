'use client'

import React from 'react'
import { RiDeleteBinLine, RiEyeLine, RiCheckLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import { type Driver } from '@/service/drivers'
import { cn } from '@/utils/classnames'

interface DriverCardProps {
  driver: Driver
}

const getProtocolIcon = (name: string) => {
  const nameLower = name.toLowerCase()
  const icons: Record<string, string> = {
    modbus: '🔌',
    onvif: '📹',
    snmp: '🌐',
    mqtt: '📡',
    bacnet: '🏢',
    opcua: '⚙️',
    simulated: '🎮',
  }
  
  for (const [key, icon] of Object.entries(icons)) {
    if (nameLower.includes(key)) {
      return icon
    }
  }
  return '🔧'
}

const CornerMark = ({ text }: { text: string }) => {
  return (
    <div className="absolute right-0 top-0 flex pl-[13px]">
      <div className="h-0 w-0 border-b-[20px] border-r-[20px] border-b-transparent border-r-background-section"></div>
      <div className="system-2xs-medium-uppercase h-5 rounded-tr-xl bg-background-section pr-2 leading-5 text-text-tertiary">{text}</div>
    </div>
  )
}

const DriverIcon = ({ name }: { name: string }) => {
  return (
    <div className="relative flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-components-button-secondary-bg text-2xl">
      {getProtocolIcon(name)}
    </div>
  )
}

const DriverTitle = ({ title }: { title: string }) => {
  return (
    <div className="system-md-semibold truncate text-text-secondary">
      {title}
    </div>
  )
}

const DriverOrgInfo = ({ 
  version,
  isLoaded,
  className 
}: { 
  version?: string
  isLoaded: boolean
  className?: string
}) => {
  return (
    <div className={cn('flex h-4 items-center space-x-0.5', className)}>
      {version && (
        <>
          <span className="system-xs-regular shrink-0 text-text-quaternary">v</span>
          <span className="system-xs-regular shrink-0 text-text-tertiary">{version}</span>
        </>
      )}
      {isLoaded && (
        <>
          {version && <span className="system-xs-regular shrink-0 text-text-quaternary">•</span>}
          <div className="flex items-center gap-1">
            <RiCheckLine className="h-3 w-3 text-text-success" />
            <span className="system-xs-regular shrink-0 text-text-success">已加载</span>
          </div>
        </>
      )}
    </div>
  )
}

const DriverDescription = ({ 
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

const DriverCard: React.FC<DriverCardProps> = ({
  driver,
}) => {
  const wrapClassName = cn(
    'group hover-bg-components-panel-on-panel-item-bg relative overflow-hidden rounded-xl border-[0.5px] border-components-panel-border bg-components-panel-on-panel-item-bg shadow-xs'
  )

  return (
    <div className={wrapClassName}>
      <div className="p-4 pb-3">
        {driver.path && <CornerMark text="动态" />}
        
        <div className="flex">
          <DriverIcon name={driver.name} />
          <div className="ml-3 w-0 grow">
            <div className="flex h-5 items-center">
              <DriverTitle title={driver.name} />
            </div>
            <DriverOrgInfo
              version={driver.version}
              isLoaded={driver.isLoaded}
              className="mt-0.5"
            />
          </div>
        </div>
        
        {driver.description && (
          <DriverDescription
            className="mt-3"
            text={driver.description}
          />
        )}

        {driver.tags && driver.tags.length > 0 && (
          <div className="mt-3 flex flex-wrap gap-1">
            {driver.tags.slice(0, 3).map((tag) => (
              <span
                key={tag}
                className="system-2xs-regular rounded bg-background-section px-1.5 py-0.5 text-text-tertiary"
              >
                {tag}
              </span>
            ))}
          </div>
        )}
      </div>

      <div className="absolute bottom-0 left-0 z-10 hidden w-full items-center gap-x-2 bg-pipeline-template-card-hover-bg p-4 pt-8 group-hover:flex">
        <Button
          variant="ghost"
          size="small"
          className="flex-1"
        >
          <RiEyeLine className="mr-1 h-4 w-4" />
          详情
        </Button>
        {driver.path && (
          <Button
            variant="ghost"
            size="small"
            className="flex-1 text-text-destructive hover:text-text-destructive"
          >
            <RiDeleteBinLine className="mr-1 h-4 w-4" />
            卸载
          </Button>
        )}
      </div>
    </div>
  )
}

export default DriverCard
