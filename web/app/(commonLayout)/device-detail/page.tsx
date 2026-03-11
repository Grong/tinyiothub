'use client'

import { useEffect, useState } from 'react'
import { useRouter } from 'next/navigation'
import DeviceDetailLayout from './components/layout-main'
import DeviceOverview from './components/overview/page'
import DeviceMonitoring from './components/monitoring/page'
import DeviceConfiguration from './components/configuration/page'

export default function DeviceDetailPage() {
  const router = useRouter()
  const [deviceId, setDeviceId] = useState<string>('')
  const [tab, setTab] = useState<string>('overview')

  useEffect(() => {
    // 解析 hash: #/xxx/overview 或 #/xxx
    const parseHash = () => {
      const hash = window.location.hash.slice(1) // 移除 #
      const match = hash.match(/^\/([^/]+)(?:\/(.+))?$/)
      
      if (match) {
        setDeviceId(match[1])
        setTab(match[2] || 'overview')
      } else {
        // 没有有效的 hash，跳转到设备列表
        router.replace('/devices')
      }
    }

    parseHash()
    
    // 监听 hash 变化
    window.addEventListener('hashchange', parseHash)
    return () => window.removeEventListener('hashchange', parseHash)
  }, [router])

  if (!deviceId) {
    return null
  }

  // 根据 tab 渲染不同的子页面
  const renderContent = () => {
    switch (tab) {
      case 'overview':
        return <DeviceOverview deviceId={deviceId} />
      case 'monitoring':
        return <DeviceMonitoring deviceId={deviceId} />
      case 'configuration':
        return <DeviceConfiguration deviceId={deviceId} />
      default:
        return <DeviceOverview deviceId={deviceId} />
    }
  }

  return (
    <DeviceDetailLayout deviceId={deviceId}>
      {renderContent()}
    </DeviceDetailLayout>
  )
}
