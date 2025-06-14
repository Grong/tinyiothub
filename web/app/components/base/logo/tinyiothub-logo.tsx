'use client'
import type { FC } from 'react'
import classNames from '@/utils/classnames'
export type LogoStyle = 'default' | 'monochromeWhite'

export const logoPathMap: Record<LogoStyle, string> = {
  default: '/logo/logo.svg',
  monochromeWhite: '/logo/logo-monochrome-white.svg',
}

export type LogoSize = 'large' | 'medium' | 'small'

export const logoSizeMap: Record<LogoSize, string> = {
  large: 'w-34 h-10 text-2xl',
  medium: 'w-26 h-8 text-xl',
  small: 'w-18 h-6 text-lg',
}

type TinyIotHubLogoProps = {
  style?: LogoStyle
  size?: LogoSize
  className?: string
}

const TinyIotHubLogo: FC<TinyIotHubLogoProps> = ({
  style = 'default',
  size = 'medium',
  className,
}) => {
  return (
    <div
      className={classNames('block object-contain', logoSizeMap[size], className)}
    >
      <h3 className='font-bold text-white'>TinyIoTHub</h3>
    </div>
  )
}

export default TinyIotHubLogo
