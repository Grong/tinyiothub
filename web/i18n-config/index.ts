export const i18n = {
  defaultLocale: 'zh-Hans',
  locales: ['en-US', 'zh-Hans'],
} as const

export type Locale = (typeof i18n)['locales'][number]