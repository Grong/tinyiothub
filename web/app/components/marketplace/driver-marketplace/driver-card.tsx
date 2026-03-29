'use client'

import React from 'react'
import { RiDownloadLine, RiEyeLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import { type DriverMetadata } from '@/service/marketplace'
import { useInstallDriver } from '@/service/marketplace'
import { cn } from '@/utils/classnames'

interface DriverCardProps {
  driver: DriverMetadata
}

// 获取协议图标
const getProtocolIcon = (protocol: string) => {
  const icons: Record<string, string> = {
    modbus: '🔌',
    onvif: '📹',
    snmp: '🌐',
    mqtt: '📡',
    bacnet: '🏢',
    opcua: '⚙️',
    others: '🔧',
  }
  return icons[protocol.toLowerCase()] || icons.others
}

// 角标组件
const CornerMark = ({ text }: { text: string }) => {
  return (
    <div className="absolute right-0 top-0 flex pl-[13px]">
      <div className="h-0 w-0 border-b-[20px] border-r-[20px] border-b-transparent border-r-background-section"></div>
      <div className="system-2xs-medium-uppercase h-5 rounded-tr-xl bg-background-section pr-2 leading-5 text-text-tertiary">{text}</div>
    </div>
  )
}

// 图标组件
const DriverIcon = ({ protocol }: { protocol: string }) => {
  return (
    <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-blue-500/20 to-indigo-500/20 flex items-center justify-center mb-4">
      {getProtocolIcon(protocol)}
    </div>
  )
}

// 标题组件
const DriverTitle = ({ title }: { title: string }) => {
  return (
    <div className="text-base font-semibold text-gray-900 mb-2">
      {title}
    </div>
  )
}

// 组织信息组件
const DriverOrgInfo = ({ 
  author, 
  protocol, 
  version,
  className 
}: { 
  author?: string
  protocol: string
  version?: string
  className?: string
}) => {
  return (
    <div className={cn('flex h-4 items-center space-x-0.5', className)}>
      <span className="system-xs-regular shrink-0 text-text-tertiary">{protocol}</span>
      {author && (
        <>
          <span className="system-xs-regular shrink-0 text-text-quaternary">/</span>
          <span className="system-xs-regular w-0 shrink-0 grow truncate text-text-tertiary">
            {author}
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

// 描述组件
const DriverDescription = ({
  text,
  className
}: {
  text: string
  className?: string
}) => {
  return (
    <div className={cn('text-sm text-gray-600 line-clamp-2', className)}>
      {text}
    </div>
  )
}

const DriverCard: React.FC<DriverCardProps> = ({
  driver,
}) => {
  const installMutation = useInstallDriver()

  const handleInstall = async () => {
    try {
      await installMutation.mutateAsync({ id: driver.id })
    } catch (error) {
      console.error('安装失败:', error)
    }
  }

  const wrapClassName = cn(
    'glass-card p-5 hover:shadow-xl transition-all duration-200 relative overflow-hidden'
  )

  return (
    <div className={wrapClassName}>
      <div className="p-4 pb-3">
        {/* 角标 */}
        {driver.license === 'MIT' && <CornerMark text="开源" />}
        
        {/* Header */}
        <div className="flex">
          <DriverIcon protocol={driver.protocol} />
          <div className="ml-3 w-0 grow">
            <div className="flex h-5 items-center">
              <DriverTitle title={driver.name} />
            </div>
            <DriverOrgInfo
              author={driver.author?.name}
              protocol={driver.protocol}
              version={driver.version}
              className="mt-0.5"
            />
          </div>
        </div>
        
        {/* 描述 */}
        {driver.description && (
          <DriverDescription
            className="mt-3"
            text={driver.description}
          />
        )}

        {/* 统计信息 */}
        <div className="mt-3 flex items-center gap-4 text-xs text-text-tertiary">
          <div className="flex items-center gap-1">
            <span>下载:</span>
            <span className="font-medium">{driver.downloads || 0}</span>
          </div>
          <div className="flex items-center gap-1">
            <span>评分:</span>
            <span className="font-medium">{driver.rating || 0}</span>
          </div>
        </div>
      </div>

      {/* 悬停显示的操作按钮 */}
      <div className="absolute bottom-0 left-0 z-10 hidden w-full items-center gap-x-2 backdrop-blur-sm bg-white/50 p-4 pt-8 group-hover:flex">
        <Button
          variant="ghost"
          size="small"
          className="flex-1"
        >
          <RiEyeLine className="mr-1 h-4 w-4" />
          详情
        </Button>
        <Button
          variant="primary"
          size="small"
          className="flex-1"
          onClick={handleInstall}
          disabled={installMutation.isPending}
        >
          <RiDownloadLine className="mr-1 h-4 w-4" />
          {installMutation.isPending ? '安装中...' : '安装'}
        </Button>
      </div>
    </div>
  )
}

export default DriverCard
