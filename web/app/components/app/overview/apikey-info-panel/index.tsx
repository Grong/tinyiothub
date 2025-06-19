'use client'
import type { FC } from 'react'
import React, { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RiCloseLine } from '@remixicon/react'
import cn from '@/utils/classnames'
import Button from '@/app/components/base/button'
import { LinkExternal02 } from '@/app/components/base/icons/src/vender/line/general'
import { useModalContext } from '@/context/modal-context'

const APIKeyInfoPanel: FC = () => {
  const isCloud = true

  const { setShowAccountSettingModal } = useModalContext()

  const { t } = useTranslation()

  const [isShow, setIsShow] = useState(true)

  if (!(isShow))
    return null

  return (
    <div className={cn('border-components-panel-border bg-components-panel-bg', 'relative mb-6 rounded-2xl border p-8 shadow-md ')}>
      <div className={cn('text-[24px] font-semibold text-text-primary', isCloud ? 'flex h-8 items-center space-x-1' : 'mb-6 leading-8')}>
        {isCloud && <em-emoji id={'😀'} />}
        {isCloud
          ? (
            <div>{t('appOverview.apiKeyInfo.cloud.trial.title', { providerName: 'OpenAI' })}</div>
          )
          : (
            <div>
              <div>{t('appOverview.apiKeyInfo.selfHost.title.row1')}</div>
              <div>{t('appOverview.apiKeyInfo.selfHost.title.row2')}</div>
            </div>
          )}
      </div>
      {isCloud && (
        <div className='mt-1 text-sm font-normal text-text-tertiary'>{t(`appOverview.apiKeyInfo.cloud.${'trial'}.description`)}</div>
      )}
      <Button
        variant='primary'
        className='mt-2 space-x-2'
        onClick={() => setShowAccountSettingModal({ payload: 'provider' })}
      >
        <div className='text-sm font-medium'>{t('appOverview.apiKeyInfo.setAPIBtn')}</div>
        <LinkExternal02 className='h-4 w-4' />
      </Button>
      <div
        onClick={() => setIsShow(false)}
        className='absolute right-4 top-4 flex h-8 w-8 cursor-pointer items-center justify-center '>
        <RiCloseLine className='h-4 w-4 text-text-tertiary' />
      </div>
    </div>
  )
}
export default React.memo(APIKeyInfoPanel)
