'use client'
import { useEffect } from 'react'
import Link from 'next/link'
import { useBoolean } from 'ahooks'
import { useSelectedLayoutSegment } from 'next/navigation'
import { Bars3Icon } from '@heroicons/react/20/solid'
import AccountDropdown from './account-dropdown'
import AppNav from './app-nav'
import EnvNav from './env-nav'
// import ExploreNav from './explore-nav'
import ToolsNav from './tools-nav'
import { WorkspaceProvider } from '@/context/workspace-context'
import TinyIotHubLogo from '@/app/components/base/logo/tinyiothub-logo'
import WorkplaceSelector from '@/app/components/header/account-dropdown/workplace-selector'
import useBreakpoints, { MediaType } from '@/hooks/use-breakpoints'
import { useGlobalPublicStore } from '@/context/global-public-context'

const navClassName = `
  flex items-center relative mr-0 sm:mr-3 px-3 h-8 rounded-xl
  font-medium text-sm
  cursor-pointer
`

const Header = () => {
  const selectedSegment = useSelectedLayoutSegment()
  const media = useBreakpoints()
  const isMobile = media === MediaType.mobile
  const [isShowNavMenu, { toggle, setFalse: hideNavMenu }] = useBoolean(false)
  const systemFeatures = useGlobalPublicStore(s => s.systemFeatures)

  useEffect(() => {
    hideNavMenu()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSegment])
  return (
    <div className='relative flex flex-1 items-center justify-between bg-background-body'>
      <div className='flex items-center'>
        {isMobile && <div
          className='flex h-8 w-8 cursor-pointer items-center justify-center'
          onClick={toggle}
        >
          <Bars3Icon className="h-4 w-4 text-gray-500" />
        </div>}
        {
          !isMobile
          && <div className='flex shrink-0 items-center gap-1.5 self-stretch pl-3'>
            <Link href="/apps" className='flex h-8 shrink-0 items-center justify-center gap-2 px-0.5'>
              {systemFeatures.branding.enabled && systemFeatures.branding.workspace_logo
                ? <img
                  src={systemFeatures.branding.workspace_logo}
                  className='block h-[22px] w-auto object-contain'
                  alt='logo'
                />
                : <TinyIotHubLogo />}
            </Link>
            <div className='font-light text-divider-deep'>/</div>
            <div className='flex items-center gap-0.5'>
              <WorkspaceProvider>
                <WorkplaceSelector />
              </WorkspaceProvider>
            </div>
          </div>
        }
      </div >
      {isMobile && (
        <div className='flex'>
          <Link href="/apps" className='mr-4 flex items-center'>
            {systemFeatures.branding.enabled && systemFeatures.branding.workspace_logo
              ? <img
                src={systemFeatures.branding.workspace_logo}
                className='block h-[22px] w-auto object-contain'
                alt='logo'
              />
              : <TinyIotHubLogo />}
          </Link>
          <div className='font-light text-divider-deep'>/</div>
        </div >
      )}
      {
        !isMobile && (
          <div className='absolute left-1/2 top-1/2 flex -translate-x-1/2 -translate-y-1/2 items-center'>
            {/* <ExploreNav className={navClassName} /> */}
            <AppNav />
            <ToolsNav className={navClassName} />
          </div>
        )
      }
      <div className='flex shrink-0 items-center pr-3'>
        <EnvNav />
        <AccountDropdown />
      </div>
      {
        (isMobile && isShowNavMenu) && (
          <div className='flex w-full flex-col gap-y-1 p-2'>
            {/* <ExploreNav className={navClassName} /> */}
            <AppNav />
            <ToolsNav className={navClassName} />
          </div>
        )
      }
    </div >
  )
}
export default Header
