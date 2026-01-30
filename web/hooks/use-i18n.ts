import { useTranslation } from 'react-i18next'
import type { Locale } from '@/i18n-config'

export const useI18n = () => {
  const { t, i18n } = useTranslation()
  
  const locale = i18n.language as Locale
  
  const changeLanguage = async (lng: Locale) => {
    await i18n.changeLanguage(lng)
  }

  return {
    t,
    locale,
    changeLanguage,
  }
}