'use client'
import { useTranslation } from 'react-i18next'
import { Fragment, useState } from 'react'
import { useRouter } from 'next/navigation'
import {
  RiAccountCircleLine,
  RiArrowRightUpLine,
  RiInformation2Line,
  RiLogoutBoxRLine,
  RiSettings3Line,
  RiTShirt2Line,
} from '@remixicon/react'
import Link from 'next/link'
import { Menu, MenuButton, MenuItem, MenuItems, Transition } from '@headlessui/react'
import Indicator from '../indicator'
import AccountAbout from '../account-about'
import Avatar from '@/app/components/base/avatar'
import ThemeSwitcher from '@/app/components/base/theme-switcher'
import { logout } from '@/service/common'
import { useAppContext } from '@/context/app-context'
import cn from '@/utils/classnames'
import { useGlobalPublicStore } from '@/context/global-public-context'
import { useModalContext } from '@/context/modal-context'

export default function AppSelector() {
  const itemClassName = `
    flex items-center w-full h-9 pl-3 pr-2 text-text-secondary system-md-regular
    rounded-lg hover:bg-state-base-hover cursor-pointer gap-1
  `
  const router = useRouter()
  const [aboutVisible, setAboutVisible] = useState(false)
  const { systemFeatures } = useGlobalPublicStore()

  const { t } = useTranslation()
  const { userProfile, langeniusVersionInfo } = useAppContext()
  const { setShowAccountSettingModal } = useModalContext()
  const handleLogout = async () => {
    await logout({
      url: '/logout',
      params: {},
    })

    localStorage.removeItem('setup_status')
    localStorage.removeItem('console_token')
    localStorage.removeItem('refresh_token')

    router.push('/signin')
  }

  return (
    <div className="">
      <Menu as="div" className="relative inline-block text-left">
        {
          ({ open }) => (
            <>
              <MenuButton className={cn('inline-flex items-center rounded-[20px] p-0.5 hover:bg-background-default-dodge', open && 'bg-background-default-dodge')}>
                <Avatar avatar={userProfile.avatar_url} name={userProfile.name} size={36} />
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
                  <MenuItem disabled>
                    <div className='flex flex-nowrap items-center py-[13px] pl-3 pr-2'>
                      <div className='grow'>
                        <div className='system-md-medium break-all text-text-primary'>
                          {userProfile.name}
                        </div>
                        <div className='system-xs-regular break-all text-text-tertiary'>{userProfile.email}</div>
                      </div>
                      <Avatar avatar={userProfile.avatar_url} name={userProfile.name} size={36} className='mr-3' />
                    </div>
                  </MenuItem>
                  <div className="px-1 py-1">
                    <MenuItem>
                      <Link
                        className={cn(itemClassName, 'group',
                          'data-[active]:bg-state-base-hover',
                        )}
                        href='/account'
                        target='_self' rel='noopener noreferrer'>
                        <RiAccountCircleLine className='size-4 shrink-0 text-text-tertiary' />
                        <div className='system-md-regular grow px-1 text-text-secondary'>{t('common.account.account')}</div>
                        <RiArrowRightUpLine className='size-[14px] shrink-0 text-text-tertiary' />
                      </Link>
                    </MenuItem>
                    <MenuItem>
                      <div className={cn(itemClassName,
                        'data-[active]:bg-state-base-hover',
                      )} onClick={() => setShowAccountSettingModal({ payload: 'members' })}>
                        <RiSettings3Line className='size-4 shrink-0 text-text-tertiary' />
                        <div className='system-md-regular grow px-1 text-text-secondary'>{t('common.userProfile.settings')}</div>
                      </div>
                    </MenuItem>
                  </div>
                  {!systemFeatures.branding.enabled && <>
                    <div className='p-1'>
                      {
                        document?.body?.getAttribute('data-public-site-about') !== 'hide' && (
                          <MenuItem>
                            <div className={cn(itemClassName, 'justify-between',
                              'data-[active]:bg-state-base-hover',
                            )} onClick={() => setAboutVisible(true)}>
                              <RiInformation2Line className='size-4 shrink-0 text-text-tertiary' />
                              <div className='system-md-regular grow px-1 text-text-secondary'>{t('common.userProfile.about')}</div>
                              <div className='flex shrink-0 items-center'>
                                <div className='system-xs-regular mr-2 text-text-tertiary'>{langeniusVersionInfo.current_version}</div>
                                <Indicator color={langeniusVersionInfo.current_version === langeniusVersionInfo.latest_version ? 'green' : 'orange'} />
                              </div>
                            </div>
                          </MenuItem>
                        )
                      }
                    </div>
                  </>}
                  <MenuItem disabled>
                    <div className='p-1'>
                      <div className={cn(itemClassName, 'hover:bg-transparent')}>
                        <RiTShirt2Line className='size-4 shrink-0 text-text-tertiary' />
                        <div className='system-md-regular grow px-1 text-text-secondary'>{t('common.theme.theme')}</div>
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
                        <div className='system-md-regular grow px-1 text-text-secondary'>{t('common.userProfile.logout')}</div>
                      </div>
                    </div>
                  </MenuItem>
                </MenuItems>
              </Transition>
            </>
          )
        }
      </Menu>
      {
        aboutVisible && <AccountAbout onCancel={() => setAboutVisible(false)} langeniusVersionInfo={langeniusVersionInfo} />
      }
    </div >
  )
}
