'use client'
import type { FC } from 'react'
import React from 'react'
import Loading from '@/app/components/base/loading'
import type { PageType } from '@/app/components/base/features/new-feature-panel/annotation-reply/type'
import { useStore as useAppStore } from '@/app/components/app/store'

type Props = {
  pageType: PageType
}

const LogAnnotation: FC<Props> = ({
  pageType,
}) => {
  const appDetail = useAppStore(state => state.appDetail)

  if (!appDetail) {
    return (
      <div className='flex h-full items-center justify-center bg-background-body'>
        <Loading />
      </div>
    )
  }

  return (
    <div className='flex h-full flex-col px-6 pt-3'>
    </div>
  )
}
export default React.memo(LogAnnotation)
