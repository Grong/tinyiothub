'use client'
import { useTranslation } from 'react-i18next'
import DeviceList from '@/app/components/devices'
import useDocumentTitle from '@/hooks/use-document-title'

const DevicesPage = () => {
  const { t } = useTranslation('device')

  useDocumentTitle(t('pageTitle'))

  return (
    <div className='flex h-full w-full flex-col overflow-hidden bg-background-body'>
      <DeviceList />
    </div>
  )
}

export default DevicesPage