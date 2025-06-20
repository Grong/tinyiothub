'use client'
import type { FC } from 'react'
import React from 'react'
import { useTranslation } from 'react-i18next'
import useDocumentTitle from '@/hooks/use-document-title'

export type IAppDetail = {
  children: React.ReactNode
}

const AppDetail: FC<IAppDetail> = ({ children }) => {
  const { t } = useTranslation()
  useDocumentTitle(t('common.menus.appDetail'))

  return (
    <>
      {children}
    </>
  )
}

export default React.memo(AppDetail)
