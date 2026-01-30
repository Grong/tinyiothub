'use client'
import { useTranslation } from 'react-i18next'
import { useEffect, useRef, useState } from 'react'
import {
  RiCloseLine,
  RiGroup2Fill,
  RiGroup2Line,
  RiSettings3Fill,
  RiSettings3Line,
  RiTranslate2,
} from '@remixicon/react'
import Button from '../../base/button'
import MembersPage from './members-page'
import LanguagePage from './language-page'
import ProfilePage from './profile-page'
import cn from '@/utils/classnames'
import useBreakpoints, { MediaType } from '@/hooks/use-breakpoints'
import MenuDialog from '@/app/components/header/account-setting/menu-dialog'
import {
  ACCOUNT_SETTING_TAB,
  type AccountSettingTab,
} from '@/app/components/header/account-setting/constants'

const iconClassName = `
  w-5 h-5 mr-2
`

type IAccountSettingProps = {
  onCancel: () => void
  activeTab?: AccountSettingTab
  onTabChange?: (tab: AccountSettingTab) => void
}

type GroupItem = {
  key: AccountSettingTab
  name: string
  description?: string
  icon: React.JSX.Element
  activeIcon: React.JSX.Element
}

export default function AccountSetting({
  onCancel,
  activeTab = ACCOUNT_SETTING_TAB.PROFILE,
  onTabChange,
}: IAccountSettingProps) {
  const [activeMenu, setActiveMenu] = useState<AccountSettingTab>(activeTab)
  useEffect(() => {
    setActiveMenu(activeTab)
  }, [activeTab])
  const { t } = useTranslation('common')

  const workplaceGroupItems: GroupItem[] = [
    {
      key: ACCOUNT_SETTING_TAB.PROFILE,
      name: t('userProfile.profile'),
      icon: <RiSettings3Line className={iconClassName} />,
      activeIcon: <RiSettings3Fill className={iconClassName} />,
    },
    {
      key: ACCOUNT_SETTING_TAB.MEMBERS,
      name: t('userProfile.members'),
      icon: <RiGroup2Line className={iconClassName} />,
      activeIcon: <RiGroup2Fill className={iconClassName} />,
    },
  ]

  const { isMobile } = useBreakpoints()

  const menuItems = [
    {
      key: 'workspace-group',
      name: t('userProfile.workspace'),
      items: workplaceGroupItems,
    },
    {
      key: 'account-group',
      name: t('common.userProfile.generalSettings'),
      items: [
        {
          key: ACCOUNT_SETTING_TAB.LANGUAGE,
          name: t('common.userProfile.language'),
          icon: <RiTranslate2 className={iconClassName} />,
          activeIcon: <RiTranslate2 className={iconClassName} />,
        },
      ],
    },
  ]
  const scrollRef = useRef<HTMLDivElement>(null)
  const [scrolled, setScrolled] = useState(false)
  useEffect(() => {
    const targetElement = scrollRef.current
    const scrollHandle = (e: Event) => {
      const userScrolled = (e.target as HTMLDivElement).scrollTop > 0
      setScrolled(userScrolled)
    }
    targetElement?.addEventListener('scroll', scrollHandle)
    return () => {
      targetElement?.removeEventListener('scroll', scrollHandle)
    }
  }, [])

  const activeItem = [...menuItems[0].items, ...menuItems[1].items].find(item => item.key === activeMenu)

  return (
    <MenuDialog
      show
      onClose={onCancel}
    >
      <div className='flex h-full'>
        <div className='flex w-[224px] flex-col border-r border-divider-subtle bg-background-default-subtle px-4 py-6'>
          <div className='title-2xl-semi-bold mb-8 text-text-primary'>{t('common.userProfile.settings')}</div>
          <div className='w-full'>
            {
              menuItems.map(menuItem => (
                <div key={menuItem.key} className='mb-6'>
                  <div className='system-xs-medium-uppercase mb-2 px-3 text-text-tertiary'>{menuItem.name}</div>
                  <div className='space-y-0.5'>
                    {
                      menuItem.items.map(item => (
                        <div
                          key={item.key}
                          className={cn(
                            'flex h-[37px] cursor-pointer items-center rounded-lg px-3 text-sm transition-colors',
                            activeMenu === item.key 
                              ? 'system-sm-semibold bg-components-menu-item-bg-active text-components-menu-item-text-active-accent border border-components-button-primary-border' 
                              : 'system-sm-medium text-components-menu-item-text hover:bg-components-menu-item-bg-hover hover:text-components-menu-item-text-hover'
                          )}
                          title={item.name}
                          onClick={() => {
                            setActiveMenu(item.key)
                            onTabChange?.(item.key)
                          }}
                        >
                          {activeMenu === item.key ? item.activeIcon : item.icon}
                          {!isMobile && <div className='truncate'>{item.name}</div>}
                        </div>
                      ))
                    }
                  </div>
                </div>
              ))
            }
          </div>
        </div>
        <div className='relative flex flex-1 bg-components-panel-bg'>
          <div className='absolute right-6 top-6 z-[9999] flex flex-col items-center'>
            <Button
              variant='ghost'
              size='medium'
              className='px-2 hover:bg-state-base-hover'
              onClick={onCancel}
            >
              <RiCloseLine className='h-5 w-5 text-text-tertiary' />
            </Button>
            <div className='system-2xs-medium-uppercase mt-1 text-text-tertiary'>ESC</div>
          </div>
          <div ref={scrollRef} className='w-full overflow-y-auto pb-4'>
            <div className={cn(
              'sticky top-0 z-20 mx-8 mb-[18px] flex items-center bg-components-panel-bg pb-4 pt-8 transition-all duration-200',
              scrolled && 'border-b border-divider-subtle'
            )}>
              <div className='title-2xl-semi-bold shrink-0 text-text-primary'>
                {activeItem?.name}
                {activeItem?.description && (
                  <div className='system-sm-regular mt-1 text-text-tertiary'>{activeItem?.description}</div>
                )}
              </div>
            </div>
            <div className='px-8 pt-2'>
              {activeMenu === 'profile' && <ProfilePage />}
              {activeMenu === 'members' && <MembersPage />}
              {activeMenu === 'language' && <LanguagePage />}
            </div>
          </div>
        </div>
      </div>
    </MenuDialog>
  )
}