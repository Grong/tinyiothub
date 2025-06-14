'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import CreateAppModal from '@/app/components/app/create-app-modal'
import { FilePlus01 } from '@/app/components/base/icons/src/vender/line/files'
import cn from '@/utils/classnames'

export type CreateAppCardProps = {
  className?: string
  onSuccess?: () => void
}

const CreateAppCard = (
  {
    ref,
    className,
    onSuccess,
  }: CreateAppCardProps & {
    ref: React.RefObject<HTMLDivElement>;
  },
) => {
  const { t } = useTranslation()

  const [showNewAppModal, setShowNewAppModal] = useState(false)

  return (
    <div
      ref={ref}
      className={cn('relative col-span-1 inline-flex h-[160px] flex-col justify-between rounded-xl border-[0.5px] border-components-card-border bg-components-card-bg', className)}
    >
      <div className='grow rounded-t-xl p-2'>
        <div className='px-6 pb-1 pt-2 text-xs font-medium leading-[18px] text-text-tertiary'>{t('app.createApp')}</div>
        <button className='mb-1 flex w-full cursor-pointer items-center rounded-lg px-6 py-[7px] text-[13px] font-medium leading-[18px] text-text-tertiary hover:bg-state-base-hover hover:text-text-secondary' onClick={() => setShowNewAppModal(true)}>
          <FilePlus01 className='mr-2 h-4 w-4 shrink-0' />
          {t('app.newApp.startFromBlank')}
        </button>
      </div>

      <CreateAppModal
        show={showNewAppModal}
        onClose={() => setShowNewAppModal(false)}
        onSuccess={() => {
          if (onSuccess)
            onSuccess()
        }}
        onCreateFromTemplate={() => {
          setShowNewAppModal(false)
        }}
      />
    </div>
  )
}

CreateAppCard.displayName = 'CreateAppCard'
export default CreateAppCard
export { CreateAppCard }
