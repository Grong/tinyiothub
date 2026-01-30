'use client'
import { useMemo } from 'react'
import cn from '@/utils/classnames'

interface AvatarProps {
  avatar?: string
  name: string
  size?: number
  className?: string
}

const Avatar = ({ avatar, name, size = 32, className }: AvatarProps) => {
  const initials = useMemo(() => {
    if (!name) return '?'
    
    const words = name.trim().split(/\s+/)
    if (words.length === 1) {
      return words[0].charAt(0).toUpperCase()
    }
    
    return (words[0].charAt(0) + words[words.length - 1].charAt(0)).toUpperCase()
  }, [name])

  const avatarStyle = {
    width: size,
    height: size,
    fontSize: Math.max(size * 0.4, 12),
  }

  if (avatar) {
    return (
      <img
        src={avatar}
        alt={name}
        className={cn(
          'rounded-full object-cover',
          className
        )}
        style={avatarStyle}
      />
    )
  }

  return (
    <div
      className={cn(
        'flex items-center justify-center rounded-full bg-gradient-to-br from-blue-500 to-purple-600 text-white font-medium',
        className
      )}
      style={avatarStyle}
    >
      {initials}
    </div>
  )
}

export default Avatar