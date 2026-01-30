'use client'
import List from './list'
import useDocumentTitle from '@/hooks/use-document-title'
import { useTranslation } from 'react-i18next'

const Devices = () => {
  const { t } = useTranslation('device')

  useDocumentTitle(t('pageTitle'))

  return (
    <div className='flex h-full w-full flex-col overflow-hidden bg-background-body'>
      <List />
    </div >
  )
}

export default Devices