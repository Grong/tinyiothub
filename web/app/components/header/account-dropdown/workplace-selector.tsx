'use client'

import React from 'react'
import { useTranslation } from 'react-i18next'

const WorkplaceSelector = () => {
  const { t } = useTranslation('common')

  return (
    <div className="flex items-center">
      <div className="text-sm font-medium text-text-primary">
        TinyIoTHub
      </div>
    </div>
  )
}

export default WorkplaceSelector