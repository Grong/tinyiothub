/**
 * 设备操作菜单组件
 * 编辑和删除操作
 */

import React from 'react'
import { useRouter } from 'next/navigation'
import { useTranslation } from 'react-i18next'
import { RiMoreFill } from '@remixicon/react'
import cn from '@/utils/classnames'
import CustomPopover from '@/app/components/base/popover'
import Divider from '@/app/components/base/divider'
import type { HtmlContentProps } from '@/app/components/base/popover'

interface DeviceActionsProps {
  deviceId: string
  onDelete: () => void
}

const DeviceActions: React.FC<DeviceActionsProps> = ({ deviceId, onDelete }) => {
  const { t } = useTranslation('device')
  const { push } = useRouter()

  const Operations = (props: HtmlContentProps) => {
    const onMouseLeave = () => {
      props.onClose?.()
    }

    const onClickEdit = (e: React.MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation()
      props.onClick?.()
      e.preventDefault()
      // 直接跳转到 device-detail 页面并设置 hash
      window.location.href = `/device-detail#/${deviceId}/configuration`
    }

    const onClickDelete = (e: React.MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation()
      props.onClick?.()
      e.preventDefault()
      onDelete()
    }

    return (
      <div className="relative flex w-full flex-col py-1" onMouseLeave={onMouseLeave}>
        <button 
          type="button" 
          className='mx-1 flex h-8 cursor-pointer items-center gap-2 rounded-lg px-3 hover:bg-state-base-hover' 
          onClick={onClickEdit}
        >
          <span className='system-sm-regular text-text-secondary'>{t('actions.editDevice')}</span>
        </button>
        <Divider className="my-1" />
        <button
          type="button"
          className='group mx-1 flex h-8 cursor-pointer items-center gap-2 rounded-lg px-3 py-[6px] hover:bg-state-destructive-hover'
          onClick={onClickDelete}
        >
          <span className='system-sm-regular text-text-secondary group-hover:text-text-destructive'>
            {t('actions.deleteDevice')}
          </span>
        </button>
      </div>
    )
  }

  return (
    <div className='!hidden shrink-0 group-hover:!flex'>
      <CustomPopover
        htmlContent={<Operations />}
        position="br"
        trigger="click"
        btnElement={
          <div className='flex h-8 w-8 cursor-pointer items-center justify-center rounded-md'>
            <RiMoreFill className='h-4 w-4 text-text-tertiary' />
          </div>
        }
        btnClassName={open =>
          cn(
            open ? '!bg-state-base-hover !shadow-none' : '!bg-transparent',
            'h-8 w-8 rounded-md border-none !p-2 hover:!bg-state-base-hover',
          )
        }
        popupClassName='!w-[216px] translate-x-[-128px]'
        className={'h-fit'}
      />
    </div>
  )
}

export default React.memo(DeviceActions)
