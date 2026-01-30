import { createInstance } from 'i18next'
import resourcesToBackend from 'i18next-resources-to-backend'
import { initReactI18next } from 'react-i18next/initReactI18next'
import { i18n } from './index'

const initI18next = async (locale: string, namespace: string) => {
  const i18nInstance = createInstance()
  await i18nInstance
    .use(initReactI18next)
    .use(
      resourcesToBackend(
        (language: string, ns: string) =>
          import(`../i18n/${language}/${ns}.ts`)
      )
    )
    .init({
      lng: locale,
      fallbackLng: i18n.defaultLocale,
      supportedLngs: i18n.locales,
      defaultNS: namespace,
      fallbackNS: namespace,
      ns: namespace,
      preload: typeof window === 'undefined' ? i18n.locales : [],
    })
  return i18nInstance
}

export default initI18next