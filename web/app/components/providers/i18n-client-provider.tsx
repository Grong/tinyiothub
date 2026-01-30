'use client'

import { useEffect, useState, createContext, useContext } from 'react'
import { I18nextProvider } from 'react-i18next'
import i18next from 'i18next'
import { initReactI18next } from 'react-i18next/initReactI18next'
import { LOCALE_COOKIE_NAME } from '@/config'

// Import all translation files
import zhHansCommon from '@/i18n/zh-Hans/common'
import enUSCommon from '@/i18n/en-US/common'
import zhHansDevice from '@/i18n/zh-Hans/device'
import enUSDevice from '@/i18n/en-US/device'
import zhHansLayout from '@/i18n/zh-Hans/layout'
import enUSLayout from '@/i18n/en-US/layout'
import zhHansLogin from '@/i18n/zh-Hans/login'
import enUSLogin from '@/i18n/en-US/login'
import zhHansTemplate from '@/i18n/zh-Hans/template'
import enUSTemplate from '@/i18n/en-US/template'

interface I18nClientProviderProps {
  children: React.ReactNode
  locale: string
}

interface I18nContextType {
  locale: string
  setLocale: (locale: string) => void
}

const I18nContext = createContext<I18nContextType | null>(null)

export const useI18nContext = () => {
  const context = useContext(I18nContext)
  if (!context) {
    throw new Error('useI18nContext must be used within I18nClientProvider')
  }
  return context
}

const I18nClientProvider = ({ children, locale: initialLocale }: I18nClientProviderProps) => {
  const [i18nInstance, setI18nInstance] = useState<typeof i18next | null>(null)
  const [currentLocale, setCurrentLocale] = useState(initialLocale)

  useEffect(() => {
    const initI18n = async () => {
      // Create a new i18next instance
      const instance = i18next.createInstance()
      
      await instance
        .use(initReactI18next)
        .init({
          lng: currentLocale,
          fallbackLng: 'zh-Hans',
          debug: process.env.NODE_ENV === 'development',
          
          // Resources
          resources: {
            'zh-Hans': {
              common: zhHansCommon,
              device: zhHansDevice,
              layout: zhHansLayout,
              login: zhHansLogin,
              template: zhHansTemplate,
            },
            'en-US': {
              common: enUSCommon,
              device: enUSDevice,
              layout: enUSLayout,
              login: enUSLogin,
              template: enUSTemplate,
            },
          },
          
          // Namespace
          defaultNS: 'common',
          ns: ['common', 'device', 'layout', 'login', 'template'],
          
          interpolation: {
            escapeValue: false, // React already escapes values
          },
          
          react: {
            useSuspense: false,
          },
        })

      setI18nInstance(instance)
    }

    initI18n()
  }, [currentLocale])

  // Update language when locale changes
  useEffect(() => {
    if (i18nInstance && i18nInstance.language !== currentLocale) {
      i18nInstance.changeLanguage(currentLocale)
    }
  }, [i18nInstance, currentLocale])

  const setLocale = (newLocale: string) => {
    setCurrentLocale(newLocale)
    
    // Save to cookie
    if (typeof document !== 'undefined') {
      document.cookie = `${LOCALE_COOKIE_NAME}=${newLocale}; path=/; max-age=31536000` // 1 year
    }
  }

  if (!i18nInstance) {
    // Return children without i18n context during initialization
    return <>{children}</>
  }

  return (
    <I18nContext.Provider value={{ locale: currentLocale, setLocale }}>
      <I18nextProvider i18n={i18nInstance}>
        {children}
      </I18nextProvider>
    </I18nContext.Provider>
  )
}

export default I18nClientProvider