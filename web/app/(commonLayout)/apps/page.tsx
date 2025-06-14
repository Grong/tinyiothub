'use client'
import { useTranslation } from 'react-i18next'
import Apps from './Apps'
import { useGlobalPublicStore } from '@/context/global-public-context'

const AppList = () => {
  const { t } = useTranslation()
  const { systemFeatures } = useGlobalPublicStore()
  return (
    <div className='relative flex h-0 shrink-0 grow flex-col overflow-y-auto bg-background-body'>
      <Apps />
      {!systemFeatures.branding.enabled && <footer className='shrink-0 grow-0 px-12 py-6'>
        <h3 className='text-gradient text-xl font-semibold leading-tight'>{t('app.join')}</h3>
        <p className='system-sm-regular mt-1 text-text-tertiary'>{t('app.communityIntro')}</p>
      </footer>}
    </div >
  )
}

export default AppList
