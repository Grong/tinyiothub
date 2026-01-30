import type { FC, PropsWithChildren } from 'react'
import { getLocaleOnServer } from '@/i18n-config/server'
import I18nClientProvider from './providers/i18n-client-provider'

const I18nServer: FC<PropsWithChildren> = async ({ children }) => {
  const locale = await getLocaleOnServer()

  return (
    <I18nClientProvider locale={locale}>
      {children}
    </I18nClientProvider>
  )
}

export default I18nServer