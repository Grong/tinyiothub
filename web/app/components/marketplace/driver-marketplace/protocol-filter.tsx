'use client'

import React from 'react'
import { 
  RiApps2Line,
  RiPlugLine,
  RiCameraLine,
  RiWifiLine,
  RiRadioLine,
  RiBuilding2Line,
  RiSettings3Line,
  RiMoreLine
} from '@remixicon/react'
import { cn } from '@/utils/classnames'
import { useDriverMarketplaceContext, type DriverProtocol } from './context'

const ProtocolFilter: React.FC = () => {
  const { selectedProtocol, setSelectedProtocol } = useDriverMarketplaceContext()

  const protocols = [
    {
      value: 'all' as DriverProtocol,
      text: '全部',
      icon: <RiApps2Line className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'modbus' as DriverProtocol,
      text: 'Modbus',
      icon: <RiPlugLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'onvif' as DriverProtocol,
      text: 'ONVIF',
      icon: <RiCameraLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'snmp' as DriverProtocol,
      text: 'SNMP',
      icon: <RiWifiLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'mqtt' as DriverProtocol,
      text: 'MQTT',
      icon: <RiRadioLine className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'bacnet' as DriverProtocol,
      text: 'BACnet',
      icon: <RiBuilding2Line className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'opcua' as DriverProtocol,
      text: 'OPC UA',
      icon: <RiSettings3Line className="mr-1.5 h-4 w-4" />,
    },
    {
      value: 'others' as DriverProtocol,
      text: '其他',
      icon: <RiMoreLine className="mr-1.5 h-4 w-4" />,
    },
  ]

  return (
    <div className="flex shrink-0 items-center justify-center space-x-2 bg-background-body py-3">
      {protocols.map(protocol => (
        <div
          key={protocol.value}
          className={cn(
            'system-md-medium flex h-8 cursor-pointer items-center rounded-xl border border-transparent px-3 text-text-tertiary hover:bg-state-base-hover hover:text-text-secondary',
            selectedProtocol === protocol.value && 'border-components-main-nav-nav-button-border !bg-components-main-nav-nav-button-bg-active !text-components-main-nav-nav-button-text-active shadow-xs',
          )}
          onClick={() => setSelectedProtocol(protocol.value)}
        >
          {protocol.icon}
          {protocol.text}
        </div>
      ))}
    </div>
  )
}

export default ProtocolFilter
