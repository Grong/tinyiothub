'use client'
import Link from 'next/link'
import { useTranslation } from 'react-i18next'
import AccountDropdown from './account-dropdown'
import AppNav from './app-nav'
import DeviceNav from './device-nav'
import AlarmNav from './alarm-nav'
import MarketplaceNav from './marketplace-nav'
import useBreakpoints from '@/hooks/use-breakpoints'

const navClassName = `
  flex items-center relative px-3 h-8 rounded-xl
  font-medium text-sm
  cursor-pointer
`

const Header = () => {
  const { isMobile } = useBreakpoints()
  const { t } = useTranslation('common')

  if (isMobile) {
    return (
      <div className='px-4 py-2'>
        <div className='flex items-center justify-between'>
          <div className='flex items-center'>
            <Link href="/dashboard" className='flex h-8 shrink-0 items-center justify-center px-0.5'>
              <div className='text-xl font-bold text-text-primary'>{t('branding.appName')}</div>
            </Link>
          </div>
          <div className='flex items-center'>
            <AccountDropdown />
          </div>
        </div>
        <div className='mt-3 flex items-center justify-center space-x-1'>
          <AppNav />
          <DeviceNav className={navClassName} />
          <AlarmNav />
          <MarketplaceNav className={navClassName} />
        </div>
      </div>
    )
  }

  return (
    <div className='flex h-[56px] items-center px-6'>
      <div className='flex min-w-0 flex-[1] items-center'>
        <Link href="/dashboard" className='flex h-8 shrink-0 items-center justify-center px-0.5'>
          <div className='text-xl font-bold text-text-primary'>{t('branding.appNameFull')}</div>
        </Link>
      </div>
      <div className='flex items-center space-x-2'>
        <AppNav />
        <DeviceNav className={navClassName} />
        <AlarmNav />
        <MarketplaceNav className={navClassName} />
      </div>
      <div className='flex min-w-0 flex-[1] items-center justify-end'>
        <AccountDropdown />
      </div>
    </div>
  )
}
export default Header