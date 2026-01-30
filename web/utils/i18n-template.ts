/**
 * 模板多语言处理工具函数
 */

import { useTranslation } from 'react-i18next'

// 多语言文本类型
export type MultiLanguageText = string | Record<string, string>

/**
 * 获取当前语言对应的文本
 * @param text 多语言文本对象或字符串
 * @param fallback 备用文本
 * @returns 当前语言的文本
 */
export function getLocalizedText(text: MultiLanguageText, fallback?: string): string {
  if (!text) return fallback || ''
  
  if (typeof text === 'string') {
    return text
  }
  
  // 获取当前语言
  const currentLanguage = getCurrentLanguage()
  
  // 尝试获取当前语言的文本
  if (text[currentLanguage]) {
    return text[currentLanguage]
  }
  
  // 尝试获取中文
  if (text['zh'] || text['zh-Hans'] || text['zh-CN']) {
    return text['zh'] || text['zh-Hans'] || text['zh-CN']
  }
  
  // 尝试获取英文
  if (text['en'] || text['en-US']) {
    return text['en'] || text['en-US']
  }
  
  // 返回第一个可用的值
  const firstValue = Object.values(text)[0]
  if (firstValue) {
    return firstValue
  }
  
  return fallback || ''
}

/**
 * 获取当前语言代码
 */
function getCurrentLanguage(): string {
  // 从localStorage获取语言设置
  if (typeof window !== 'undefined') {
    const savedLanguage = localStorage.getItem('language')
    if (savedLanguage) {
      return savedLanguage
    }
  }
  
  // 从浏览器语言获取
  if (typeof navigator !== 'undefined') {
    const browserLanguage = navigator.language
    if (browserLanguage.startsWith('zh')) {
      return 'zh'
    }
    if (browserLanguage.startsWith('en')) {
      return 'en'
    }
  }
  
  // 默认返回中文
  return 'zh'
}

/**
 * React Hook: 获取本地化文本
 */
export function useLocalizedText() {
  const { i18n } = useTranslation()
  
  return (text: MultiLanguageText, fallback?: string): string => {
    if (!text) return fallback || ''
    
    if (typeof text === 'string') {
      return text
    }
    
    // 获取当前i18n语言
    const currentLanguage = i18n.language
    
    // 映射语言代码
    const languageMap: Record<string, string[]> = {
      'zh-Hans': ['zh', 'zh-Hans', 'zh-CN'],
      'zh': ['zh', 'zh-Hans', 'zh-CN'],
      'en-US': ['en', 'en-US'],
      'en': ['en', 'en-US'],
    }
    
    const possibleKeys = languageMap[currentLanguage] || [currentLanguage, 'zh', 'en']
    
    // 尝试按优先级获取文本
    for (const key of possibleKeys) {
      if (text[key]) {
        return text[key]
      }
    }
    
    // 返回第一个可用的值
    const firstValue = Object.values(text)[0]
    if (firstValue) {
      return firstValue
    }
    
    return fallback || ''
  }
}

/**
 * 获取模板显示名称
 */
export function getTemplateDisplayName(template: any): string {
  const getLocalizedText = useLocalizedText()
  return getLocalizedText(template.displayName || template.display_name, template.name)
}

/**
 * 获取模板描述
 */
export function getTemplateDescription(template: any): string {
  const getLocalizedText = useLocalizedText()
  return getLocalizedText(template.description, '')
}

/**
 * 获取属性显示名称
 */
export function getPropertyDisplayName(property: any): string {
  const getLocalizedText = useLocalizedText()
  return getLocalizedText(property.displayName || property.display_name, property.name)
}

/**
 * 获取属性描述
 */
export function getPropertyDescription(property: any): string {
  const getLocalizedText = useLocalizedText()
  return getLocalizedText(property.description, '')
}

/**
 * 获取命令显示名称
 */
export function getCommandDisplayName(command: any): string {
  const getLocalizedText = useLocalizedText()
  return getLocalizedText(command.displayName || command.display_name, command.name)
}

/**
 * 获取命令描述
 */
export function getCommandDescription(command: any): string {
  const getLocalizedText = useLocalizedText()
  return getLocalizedText(command.description, '')
}

/**
 * 获取分类图标
 */
export function getCategoryIcon(category: string): string {
  const iconMap: Record<string, string> = {
    sensors: '🌡️',
    cameras: '📷',
    controllers: '🎛️',
    robots: '🤖',
    actuators: '⚡',
    gateways: '🌐',
    default: '📱'
  }
  return iconMap[category] || iconMap.default
}