'use client'
import React, { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RiCheckLine, RiGlobalLine } from '@remixicon/react'
import Button from '@/app/components/base/button'
import { useI18nContext } from '@/app/components/providers/i18n-client-provider'
import cn from '@/utils/classnames'

interface Language {
  code: string
  name: string
  nativeName: string
  flag: string
}

const languages: Language[] = [
  {
    code: 'zh-Hans',
    name: 'Chinese (Simplified)',
    nativeName: '简体中文',
    flag: '🇨🇳',
  },
  {
    code: 'en-US',
    name: 'English (US)',
    nativeName: 'English',
    flag: '🇺🇸',
  },
]

const LanguagePage = () => {
  const { t } = useTranslation('common')
  const { locale, setLocale } = useI18nContext()
  const [selectedLanguage, setSelectedLanguage] = useState(locale)
  const [isLoading, setIsLoading] = useState(false)

  const handleLanguageChange = async (languageCode: string) => {
    setSelectedLanguage(languageCode)
  }

  const handleSave = async () => {
    if (selectedLanguage === locale) {
      return
    }

    setIsLoading(true)
    try {
      setLocale(selectedLanguage)
      
      // 设置文档语言属性
      if (typeof document !== 'undefined') {
        document.documentElement.lang = selectedLanguage
      }
      
    } catch (error) {
      console.error('Failed to change language:', error)
    } finally {
      setIsLoading(false)
    }
  }

  const currentLanguage = languages.find(lang => lang.code === locale) || languages[0]

  return (
    <div className="max-w-2xl">
      {/* 当前语言显示 */}
      <div className="mb-8">
        <div className="system-lg-semibold text-text-primary mb-2">{t('language.settings')}</div>
        <div className="system-sm-regular text-text-tertiary mb-4">
          {t('language.selectPreferred')}
        </div>
        
        <div className="bg-components-menu-item-bg-active border border-components-button-primary-border rounded-lg p-4 flex items-center space-x-3">
          <RiGlobalLine className="w-5 h-5 text-text-accent" />
          <div>
            <div className="system-sm-semibold text-text-primary">
              {t('language.current')}: {currentLanguage.flag} {currentLanguage.nativeName}
            </div>
            <div className="system-xs-regular text-text-secondary">
              {currentLanguage.name}
            </div>
          </div>
        </div>
      </div>

      {/* 语言选择列表 */}
      <div className="space-y-4 mb-8">
        <div className="system-md-semibold text-text-primary mb-4">{t('language.selectLanguage')}</div>
        
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {languages.map((language) => (
            <div
              key={language.code}
              className={cn(
                'relative border rounded-lg p-4 cursor-pointer transition-all duration-200',
                selectedLanguage === language.code
                  ? 'border-components-button-primary-border bg-components-menu-item-bg-active ring-2 ring-components-button-primary-border'
                  : 'border-divider-subtle hover:border-divider-regular hover:bg-state-base-hover'
              )}
              onClick={() => handleLanguageChange(language.code)}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <span className="text-2xl">{language.flag}</span>
                  <div>
                    <div className={cn(
                      'system-sm-semibold',
                      selectedLanguage === language.code ? 'text-text-accent' : 'text-text-primary'
                    )}>
                      {language.nativeName}
                    </div>
                    <div className={cn(
                      'system-xs-regular',
                      selectedLanguage === language.code ? 'text-text-accent-secondary' : 'text-text-tertiary'
                    )}>
                      {language.name}
                    </div>
                  </div>
                </div>
                
                {selectedLanguage === language.code && (
                  <RiCheckLine className="w-5 h-5 text-text-accent" />
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* 保存按钮 */}
      <div className="flex justify-end space-x-3 pt-6 border-t border-divider-subtle">
        <Button
          variant="secondary"
          size="medium"
          onClick={() => setSelectedLanguage(locale)}
          disabled={selectedLanguage === locale}
        >
          {t('actions.reset')}
        </Button>
        <Button
          variant="primary"
          size="medium"
          loading={isLoading}
          onClick={handleSave}
          disabled={selectedLanguage === locale}
        >
          {t('actions.saveChanges')}
        </Button>
      </div>

      {/* 语言说明 */}
      <div className="mt-8 bg-components-panel-bg-alt border border-divider-subtle rounded-lg p-4">
        <div className="system-sm-semibold text-text-primary mb-2">{t('language.supportNote')}</div>
        <div className="system-xs-regular text-text-tertiary space-y-1">
          <div>• {t('language.immediateEffect')}</div>
          <div>• {t('language.technicalTerms')}</div>
          <div>• {t('language.contactAdmin')}</div>
        </div>
      </div>
    </div>
  )
}

export default LanguagePage