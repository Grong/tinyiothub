'use client'
import type { FC } from 'react'
import React from 'react'
import { useTranslation } from 'react-i18next'
import ToolProviderList from '@/app/components/tools/provider-list'
import useDocumentTitle from '@/hooks/use-document-title'
const ToolsList: FC = () => {
  const { t } = useTranslation()
  useDocumentTitle(t('common.menus.tools'))

  return <ToolProviderList />
}
export default React.memo(ToolsList)
