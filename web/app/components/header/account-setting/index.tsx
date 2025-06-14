'use client'
import { useTranslation } from 'react-i18next'
import { useEffect, useRef, useState } from 'react'
import {
  RiCloseLine,
  RiColorFilterFill,
  RiColorFilterLine,
  RiGroup2Fill,
  RiGroup2Line,
  RiTranslate2,
} from '@remixicon/react'
import Button from '../../base/button'
import MembersPage from './members-page'
import LanguagePage from './language-page'
import cn from '@/utils/classnames'
import CustomPage from '@/app/components/custom/custom-page'
import useBreakpoints, { MediaType } from '@/hooks/use-breakpoints'
import MenuDialog from '@/app/components/header/account-setting/menu-dialog'
import Input from '@/app/components/base/input'

const iconClassName = `
  w-5 h-5 mr-2
`

type IAccountSettingProps = {
  onCancel: () => void
  activeTab?: string
}

type GroupItem = {
  key: string
  name: string
  description?: string
  icon: React.JSX.Element
  activeIcon: React.JSX.Element
}

export default function AccountSetting({
  onCancel,
  activeTab = 'members',
}: IAccountSettingProps) {
  const [activeMenu, setActiveMenu] = useState(activeTab)
  const { t } = useTranslation()

  const workplaceGroupItems = (() => {
    return [
      {
        key: 'members',
        name: t('common.settings.members'),
        icon: <RiGroup2Line className={iconClassName} />,
        activeIcon: <RiGroup2Fill className={iconClassName} />,
      },
      {
        key: 'custom',
        name: t('custom.custom'),
        icon: <RiColorFilterLine className={iconClassName} />,
        activeIcon: <RiColorFilterFill className={iconClassName} />,
      },
    ].filter(item => !!item.key) as GroupItem[]
  })()

  const media = useBreakpoints()
  const isMobile = media === MediaType.mobile

  const menuItems = [
    {
      key: 'workspace-group',
      name: t('common.settings.workplaceGroup'),
      items: workplaceGroupItems,
    },
    {
      key: 'account-group',
      name: t('common.settings.generalGroup'),
      items: [
        {
          key: 'language',
          name: t('common.settings.language'),
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

  const [searchValue, setSearchValue] = useState<string>('')

  return (
    <MenuDialog
      show
      onClose={onCancel}
    >
      <div className='mx-auto flex h-[100vh] max-w-[1048px]'>
        <div className='flex w-[44px] flex-col border-r border-divider-burn pl-4 pr-6 sm:w-[224px]'>
          <div className='title-2xl-semi-bold mb-8 mt-6 px-3 py-2 text-text-primary'>{t('common.userProfile.settings')}</div>
          <div className='w-full'>
            {
              menuItems.map(menuItem => (
                <div key={menuItem.key} className='mb-2'>
                  <div className='system-xs-medium-uppercase mb-0.5 py-2 pb-1 pl-3 text-text-tertiary'>{menuItem.name}</div>
                  <div>
                    {
                      menuItem.items.map(item => (
                        <div
                          key={item.key}
                          className={cn(
                            'mb-0.5 flex h-[37px] cursor-pointer items-center rounded-lg p-1 pl-3 text-sm',
                            activeMenu === item.key ? 'system-sm-semibold bg-state-base-active text-components-menu-item-text-active' : 'system-sm-medium text-components-menu-item-text')}
                          title={item.name}
                          onClick={() => setActiveMenu(item.key)}
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
        <div className='relative flex w-[824px]'>
          <div className='absolute -right-11 top-6 z-[9999] flex flex-col items-center'>
            <Button
              variant='tertiary'
              size='large'
              className='px-2'
              onClick={onCancel}
            >
              <RiCloseLine className='h-5 w-5' />
            </Button>
            <div className='system-2xs-medium-uppercase mt-1 text-text-tertiary'>ESC</div>
          </div>
          <div ref={scrollRef} className='w-full overflow-y-auto bg-components-panel-bg pb-4'>
            <div className={cn('sticky top-0 z-20 mx-8 mb-[18px] flex items-center bg-components-panel-bg pb-2 pt-[27px]', scrolled && 'border-b border-divider-regular')}>
              <div className='title-2xl-semi-bold shrink-0 text-text-primary'>
                {activeItem?.name}
                {activeItem?.description && (
                  <div className='system-sm-regular mt-1 text-text-tertiary'>{activeItem?.description}</div>
                )}
              </div>
              {activeItem?.key === 'provider' && (
                <div className='flex grow justify-end'>
                  <Input
                    showLeftIcon
                    wrapperClassName='!w-[200px]'
                    className='!h-8 !text-[13px]'
                    onChange={e => setSearchValue(e.target.value)}
                    value={searchValue}
                  />
                </div>
              )}
            </div>
            <div className='px-4 pt-2 sm:px-8'>
              {activeMenu === 'members' && <MembersPage />}
              {activeMenu === 'custom' && <CustomPage />}
              {activeMenu === 'language' && <LanguagePage />}
            </div>
          </div>
        </div>
      </div>
    </MenuDialog>
  )
}
