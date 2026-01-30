/**
 * 设备标签组件
 * 显示和管理设备标签
 */

import React from 'react'
import cn from '@/utils/classnames'
import TagSelector from '@/app/components/base/tag-management/selector'
import type { Tag } from '@/app/components/base/tag-management/constant'

interface DeviceTagsProps {
  deviceId: string
  tags: Tag[]
  onTagsUpdate: (tags: Tag[]) => void
  onRefresh?: () => void
}

const DeviceTags: React.FC<DeviceTagsProps> = ({ 
  deviceId, 
  tags, 
  onTagsUpdate, 
  onRefresh 
}) => {
  return (
    <div 
      className={cn('flex w-0 grow items-center gap-1')} 
      onClick={(e) => {
        e.stopPropagation()
        e.preventDefault()
      }}
    >
      <div className='mr-[41px] w-full grow group-hover:!mr-0'>
        <TagSelector
          position='bl'
          type='device'
          targetID={deviceId}
          value={tags.map(tag => tag.id)}
          selectedTags={tags}
          onCacheUpdate={onTagsUpdate}
          onChange={onRefresh}
        />
      </div>
    </div>
  )
}

export default React.memo(DeviceTags)
