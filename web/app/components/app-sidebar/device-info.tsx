import { useTranslation } from 'react-i18next'
import { useRouter } from 'next/navigation'
import React, { useCallback, useState } from 'react'
import {
  RiDeleteBinLine,
  RiEditLine,
  RiEqualizer2Line,
  RiFileCopy2Line,
} from '@remixicon/react'
import { useStore as useDeviceStore } from '@/app/components/device/store'
import { useToast } from '@/hooks/use-toast'
import { useAppContext } from '@/context/app-context'
import { useDeleteDevice } from '@/service/devices'
import ContentDialog from '@/app/components/base/content-dialog'
import type { Operation } from './device-operations'
import DeviceOperations from './device-operations'
import dynamic from 'next/dynamic'
import cn from '@/utils/classnames'

const Confirm = dynamic(() => import('@/app/components/base/confirm'), {
  ssr: false,
})

export type IDeviceInfoProps = {
  expand: boolean
  onlyShowDetail?: boolean
  openState?: boolean
  onDetailExpand?: (expand: boolean) => void
}

const DeviceInfo = ({ expand, onlyShowDetail = false, openState = false, onDetailExpand }: IDeviceInfoProps) => {
  const { t } = useTranslation('device')
  const { toast } = useToast()
  const { replace } = useRouter()
  const deviceDetail = useDeviceStore(state => state.deviceDetail)
  const setDeviceDetail = useDeviceStore(state => state.setDeviceDetail)
  const [open, setOpen] = useState(openState)
  const [showConfirmDelete, setShowConfirmDelete] = useState(false)
  const deleteDeviceMutation = useDeleteDevice()

  const onConfirmDelete = useCallback(async () => {
    if (!deviceDetail)
      return
    try {
      await deleteDeviceMutation.mutateAsync(deviceDetail.id)
      toast.success(t('deviceDeleted'))
      setDeviceDetail()
      replace('/devices')
    }
    catch (e: any) {
      toast.error(`${t('deviceDeleteFailed')}${'message' in e ? `: ${e.message}` : ''}`)
    }
    setShowConfirmDelete(false)
  }, [deviceDetail, toast, replace, setDeviceDetail, t, deleteDeviceMutation])

  const { isCurrentWorkspaceEditor } = useAppContext()

  if (!deviceDetail)
    return null

  const primaryOperations = [
    {
      id: 'edit',
      title: t('editDevice'),
      icon: <RiEditLine />,
      onClick: () => {
        setOpen(false)
        onDetailExpand?.(false)
        replace(`/device/${deviceDetail.id}/configuration`)
      },
    },
    {
      id: 'duplicate',
      title: t('duplicate'),
      icon: <RiFileCopy2Line />,
      onClick: () => {
        setOpen(false)
        onDetailExpand?.(false)
        // TODO: Implement device duplication
        toast.info(t('duplicateNotImplemented'))
      },
    },
  ]

  const secondaryOperations: Operation[] = [
    // Delete operation
    {
      id: 'delete',
      title: t('operation.delete'),
      icon: <RiDeleteBinLine />,
      onClick: () => {
        setOpen(false)
        onDetailExpand?.(false)
        setShowConfirmDelete(true)
      },
    },
  ]

  const getDeviceStatusText = (state?: number) => {
    switch (state) {
      case 1:
        return t('status.online')
      case 2:
        return t('status.error')
      case 3:
        return t('status.maintenance')
      case 0:
      default:
        return t('status.offline')
    }
  }

  return (
    <div>
      {!onlyShowDetail && (
        <button type="button"
          onClick={() => {
            if (isCurrentWorkspaceEditor)
              setOpen(v => !v)
          }}
          className='block w-full'
        >
          <div className='flex flex-col gap-2 rounded-lg p-1 hover:bg-state-base-hover'>
            <div className='flex items-center gap-1'>
              <div className={cn(!expand && 'ml-1')}>
                <div className={cn(
                  'flex items-center justify-center rounded-lg border border-divider-subtle',
                  expand ? 'h-10 w-10' : 'h-8 w-8'
                )}>
                  <RiEqualizer2Line className={cn(
                    'text-text-tertiary',
                    expand ? 'h-6 w-6' : 'h-4 w-4'
                  )} />
                </div>
              </div>
              {expand && (
                <div className='ml-auto flex items-center justify-center rounded-md p-0.5'>
                  <div className='flex h-5 w-5 items-center justify-center'>
                    <RiEqualizer2Line className='h-4 w-4 text-text-tertiary' />
                  </div>
                </div>
              )}
            </div>
            {!expand && (
              <div className='flex items-center justify-center'>
                <div className='flex h-5 w-5 items-center justify-center rounded-md p-0.5'>
                  <RiEqualizer2Line className='h-4 w-4 text-text-tertiary' />
                </div>
              </div>
            )}
            {expand && (
              <div className='flex flex-col items-start gap-1'>
                <div className='flex w-full'>
                  <div className='system-md-semibold truncate whitespace-nowrap text-text-secondary'>{deviceDetail.name}</div>
                </div>
                <div className='system-2xs-medium-uppercase whitespace-nowrap text-text-tertiary'>
                  {getDeviceStatusText(deviceDetail.state)}
                </div>
              </div>
            )}
          </div>
        </button>
      )}
      <ContentDialog
        show={onlyShowDetail ? openState : open}
        onClose={() => {
          setOpen(false)
          onDetailExpand?.(false)
        }}
        className='absolute bottom-2 left-2 top-2 flex w-[420px] flex-col rounded-2xl !p-0'
      >
        <div className='flex shrink-0 flex-col items-start justify-center gap-3 self-stretch p-4'>
          <div className='flex items-center gap-3 self-stretch'>
            <div className='flex h-10 w-10 items-center justify-center rounded-lg border border-divider-subtle'>
              <RiEqualizer2Line className='h-6 w-6 text-text-tertiary' />
            </div>
            <div className='flex flex-1 flex-col items-start justify-center overflow-hidden'>
              <div className='system-md-semibold w-full truncate text-text-secondary'>{deviceDetail.name}</div>
              <div className='system-2xs-medium-uppercase text-text-tertiary'>{getDeviceStatusText(deviceDetail.state)}</div>
            </div>
          </div>
          {/* description */}
          {deviceDetail.description && (
            <div className='system-xs-regular overflow-wrap-anywhere max-h-[105px] w-full max-w-full overflow-y-auto whitespace-normal break-words text-text-tertiary'>{deviceDetail.description}</div>
          )}
          {/* operations */}
          <DeviceOperations
            gap={4}
            primaryOperations={primaryOperations}
            secondaryOperations={secondaryOperations}
          />
        </div>
      </ContentDialog>
      {showConfirmDelete && (
        <Confirm
          title={t('deleteDeviceConfirmTitle')}
          content={t('deleteDeviceConfirmContent')}
          isShow={showConfirmDelete}
          onConfirm={onConfirmDelete}
          onCancel={() => setShowConfirmDelete(false)}
        />
      )}
    </div>
  )
}

export default React.memo(DeviceInfo)