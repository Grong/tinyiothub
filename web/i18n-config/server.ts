import { LOCALE_COOKIE_NAME } from '@/config'
import type { Locale } from './index'
import { i18n } from './index'

export const getLocaleOnServer = async (): Promise<Locale> => {
  // 静态导出模式：直接返回默认语言
  return i18n.defaultLocale
}