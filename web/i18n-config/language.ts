export const languages = [
  {
    value: 'en-US',
    name: 'English',
    example: 'Hello',
  },
  {
    value: 'zh-Hans',
    name: '简体中文',
    example: '你好',
  },
]

export const LanguagesSupported = languages.map(item => item.value)

// Get language for documentation
export const getDocLanguage = (locale: string): string => {
  // Map locale to documentation language
  switch (locale) {
    case 'zh-Hans':
      return 'zh'
    case 'en-US':
    default:
      return 'en'
  }
}

// Get language code
export const getLanguage = (locale: string): string => {
  return locale || 'en-US'
}

// Get language for pricing page
export const getPricingPageLanguage = (locale: string): string => {
  // Map locale to pricing page language
  switch (locale) {
    case 'zh-Hans':
      return 'zh-CN'
    case 'en-US':
    default:
      return 'en'
  }
}