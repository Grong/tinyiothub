import data from './languages.json'
export type Item = {
  value: number | string
  name: string
  example: string
}

export type I18nText = {
  'en-US': string
  'zh-Hans': string
  'pt-BR': string
  'es-ES': string
  'fr-FR': string
  'de-DE': string
  'ja-JP': string
  'ko-KR': string
  'ru-RU': string
  'it-IT': string
  'uk-UA': string
  'vi-VN': string
  'de_DE': string
  'zh_Hant': string
  'ro-RO': string
  'pl-PL': string
  'hi-IN': string
  'fa-IR': string
  'sl-SI': string
  'th-TH': string
}

export const languages = data.languages

export const LanguagesSupported = languages.filter(item => item.supported).map(item => item.value)

export const getLanguage = (locale: string) => {
  if (['zh-Hans', 'ja-JP'].includes(locale))
    return locale.replace('-', '_')

  return LanguagesSupported[0].replace('-', '_')
}

const DOC_LANGUAGE: Record<string, string> = {
  'zh-Hans': 'zh-hans',
  'ja-JP': 'ja-jp',
  'en-US': 'en',
}

export const getDocLanguage = (locale: string) => {
  return DOC_LANGUAGE[locale] || 'en'
}

const PRICING_PAGE_LANGUAGE: Record<string, string> = {
  'ja-JP': 'jp',
}

export const getPricingPageLanguage = (locale: string) => {
  return PRICING_PAGE_LANGUAGE[locale] || ''
}

export const NOTICE_I18N = {
  title: {
    en_US: 'Important Notice',
    zh_Hans: '重要公告',
  },
  desc: {
    en_US:
      '',
    zh_Hans:
      '',
  },
  href: '#',
}
