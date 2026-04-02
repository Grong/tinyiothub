import type { Viewport } from 'next'
import I18nServer from './components/i18n-server'
import BrowserInitializer from './components/browser-initializer'
import { getLocaleOnServer } from '@/i18n-config/server'
import { TanstackQueryInitializer } from '@/context/query-client'
import { ThemeProvider } from 'next-themes'
import './styles/globals.css'
import GlobalPublicStoreProvider from '@/context/global-public-context'
import { StoreProvider } from '@/store/provider'
import { AppProvider } from '@/context/app-context'
import { ToastContainer } from './components/base/toast'
import { DatasetAttr } from '@/types/feature'
import cn from '@/utils/classnames'

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  maximumScale: 1,
  viewportFit: 'cover',
  userScalable: false,
}

export const metadata = {
  title: 'TinyIoTHub',
  description: '轻量级、高性能、企业级的物联网边缘网关系统',
  icons: {
    icon: '/logo.svg',
  },
}

const LocaleLayout = async ({
  children,
}: {
  children: React.ReactNode
}) => {
  const locale = await getLocaleOnServer()

  const datasetMap: Record<DatasetAttr, string | undefined> = {
    [DatasetAttr.DATA_API_PREFIX]: process.env.NEXT_PUBLIC_API_PREFIX,
    [DatasetAttr.DATA_PUBLIC_API_PREFIX]: process.env.NEXT_PUBLIC_PUBLIC_API_PREFIX,
    [DatasetAttr.DATA_PUBLIC_EDITION]: process.env.NEXT_PUBLIC_EDITION,
    [DatasetAttr.DATA_PUBLIC_SITE_ABOUT]: process.env.NEXT_PUBLIC_SITE_ABOUT,
  }

  return (
    <html lang={locale ?? 'en'} className={cn('h-full')} suppressHydrationWarning>
      <head>
        <meta name="theme-color" content="#1C64F2" />
        <meta name="mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-status-bar-style" content="default" />
        <meta name="apple-mobile-web-app-title" content="TinyIoTHub" />
        <link rel="icon" type="image/svg+xml" href="/logo.svg" />
        <meta name="msapplication-TileColor" content="#1C64F2" />
      </head>
      <body
        className='color-scheme h-full select-auto'
        {...datasetMap}
      >
        <ThemeProvider
          attribute='data-theme'
          defaultTheme='system'
          enableSystem
          disableTransitionOnChange
          enableColorScheme={false}
        >
          <TanstackQueryInitializer>
            <I18nServer>
              <StoreProvider>
                <GlobalPublicStoreProvider>
                  <AppProvider>
                    <BrowserInitializer>
                      {children}
                      <ToastContainer />
                    </BrowserInitializer>
                  </AppProvider>
                </GlobalPublicStoreProvider>
              </StoreProvider>
            </I18nServer>
          </TanstackQueryInitializer>
        </ThemeProvider>
      </body>
    </html>
  )
}

export default LocaleLayout