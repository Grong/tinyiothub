'use client'

import React from 'react'

const Description: React.FC = () => {
  return (
    <div className="bg-background-body px-12 py-6">
      <div className="mx-auto max-w-4xl text-center">
        <h1 className="title-2xl-semi-bold mb-2 text-text-primary">
          驱动程序市场
        </h1>
        <p className="system-md-regular text-text-secondary">
          发现和安装各种设备驱动程序，扩展您的IoT平台功能。支持Modbus、ONVIF、SNMP等多种协议，轻松连接各类设备。
        </p>
      </div>
    </div>
  )
}

export default Description
