'use client'
import { useTranslation } from 'react-i18next'
import { Fragment, useState } from 'react'
import { useRouter } from 'next/navigation'
import {
  RiAccountCircleLine,
  RiLogoutBoxRLine,
  RiSettings3Line,
  RiTShirt2Line,
} from '@remixicon/react'
import { Menu, MenuButton, MenuItem, MenuItems, Transition } from '@headlessui/react'
import Avatar from '@/app/components/base/avatar'
import ThemeSwitcher from '@/app/components/base/theme-switcher'
import AccountSetting from '@/app/components/header/account-setting'
import { useAuthStore } from '@/store/provider'
import cn from '@/utils/classnames'

export default function AccountDropdown() {
  const itemClassName = `
    flex items-center w-full h-8 pl-3 pr-2 text-text-secondary system-md-regular
    rounded-lg hover:bg-state-base-hover cursor-pointer gap-1
  `
  const router = useRouter()
  const { t } = useTranslation('common')
  const { user, logout } = useAuthStore()
  const [showAccountSetting, setShowAccountSetting] = useState(false)

  // Temporary mock user for testing
  const mockUser = user || {
    id: '1',
    name: 'Test User',
    email: 'test@example.com',
    avatar: '',
    phone: '',
    dateLastLogon: '',
    isDisabled: 0,
  }

  const handleLogout = async () => {
    logout()
    router.push('/signin')
  }

  const handleOpenSettings = () => {
    setShowAccountSetting(true)
  }

  if (!mockUser) return null

  return (
    <>
      <div className="">
        <Menu as="div" className="relative inline-block text-left">
          {
            ({ open }) => (
              <>
                <MenuButton className={cn('inline-flex items-center rounded-[20px] p-0.5 hover:bg-background-default-dodge', open && 'bg-background-default-dodge')}>
                  <Avatar avatar={mockUser.avatar} name={mockUser.name} size={36} />
                </MenuButton>
                <Transition
                  as={Fragment}
                  enter="transition ease-out duration-100"
                  enterFrom="transform opacity-0 scale-95"
                  enterTo="transform opacity-100 scale-100"
                  leave="transition ease-in duration-75"
                  leaveFrom="transform opacity-100 scale-100"
                  leaveTo="transform opacity-0 scale-95"
                >
                  <MenuItems
                    className="
                      absolute right-0 mt-1.5 w-60 max-w-80
                      origin-top-right divide-y divide-divider-subtle rounded-xl bg-components-panel-bg-blur shadow-lg
                      backdrop-blur-sm focus:outline-none
                    "
                  >
                    <div className="px-1 py-1">
                      <MenuItem disabled>
                        <div className='flex flex-nowrap items-center py-2 pl-3 pr-2'>
                          <div className='grow'>
                            <div className='system-md-medium break-all text-text-primary'>
                              {mockUser.name}
                            </div>
                            <div className='system-xs-regular break-all text-text-tertiary'>{mockUser.email}</div>
                          </div>
                          <Avatar avatar={mockUser.avatar} name={mockUser.name} size={36} />
                        </div>
                      </MenuItem>
                      <MenuItem>
                        <div className={cn(itemClassName,
                          'data-[active]:bg-state-base-hover',
                        )}>
                          <RiAccountCircleLine className='size-4 shrink-0 text-text-tertiary' />
                          <div className='system-md-regular grow px-1 text-text-secondary'>{t('account.account')}</div>
                        </div>
                      </MenuItem>
                      <MenuItem>
                        <div 
                          className={cn(itemClassName,
                            'data-[active]:bg-state-base-hover',
                          )}
                          onClick={handleOpenSettings}
                        >
                          <RiSettings3Line className='size-4 shrink-0 text-text-tertiary' />
                          <div className='system-md-regular grow px-1 text-text-secondary'>{t('userProfile.settings')}</div>
                        </div>
                      </MenuItem>
                    </div>
                    <MenuItem disabled>
                      <div className='p-1'>
                        <div className={cn(itemClassName, 'hover:bg-transparent')}>
                          <RiTShirt2Line className='size-4 shrink-0 text-text-tertiary' />
                          <div className='system-md-regular grow px-1 text-text-secondary'>{t('theme.theme')}</div>
                          <ThemeSwitcher />
                        </div>
                      </div>
                    </MenuItem>
                    <MenuItem>
                      <div className='p-1' onClick={() => handleLogout()}>
                        <div
                          className={cn(itemClassName, 'group justify-between',
                            'data-[active]:bg-state-base-hover',
                          )}
                        >
                          <RiLogoutBoxRLine className='size-4 shrink-0 text-text-tertiary' />
                          <div className='system-md-regular grow px-1 text-text-secondary'>{t('userProfile.logout')}</div>
                        </div>
                      </div>
                    </MenuItem>
                  </MenuItems>
                </Transition>
              </>
            )
          }
        </Menu>
      </div>

      {/* 账户设置弹出框 */}
      {showAccountSetting && (
        <AccountSetting
          onCancel={() => setShowAccountSetting(false)}
        />
      )}
    </>
  )
}