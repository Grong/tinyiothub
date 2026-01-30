'use client'

import React from 'react'
import { RiAddLine, RiUploadLine } from '@remixicon/react'
import Button from '@/app/components/base/button'

const MyTemplates: React.FC = () => {
  return (
    <div className="flex grow flex-col bg-components-panel-bg">
      {/* 空状态 */}
      <div className="flex flex-1 flex-col items-center justify-center px-12 py-16">
        <div className="text-6xl">📝</div>
        <div className="mt-4 text-lg font-medium text-text-secondary">
          您还没有创建任何模板
        </div>
        <div className="mt-2 text-sm text-text-tertiary">
          创建自定义设备模板，或从文件导入现有模板
        </div>
        
        <div className="mt-8 flex gap-3">
          <Button variant="primary">
            <RiAddLine className="mr-1 h-4 w-4" />
            创建模板
          </Button>
          <Button variant="secondary">
            <RiUploadLine className="mr-1 h-4 w-4" />
            导入模板
          </Button>
        </div>
      </div>
    </div>
  )
}

export default MyTemplates