'use client'

import { useTranslation } from 'react-i18next'

export default function SettingsPage() {
  const { t } = useTranslation('common')

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold text-text-primary">{t('pages.settings.title')}</h1>
        <p className="mt-1 text-sm text-text-secondary">
          {t('pages.settings.subtitle')}
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
          <div className="px-4 py-5 sm:p-6">
            <h3 className="text-lg leading-6 font-medium text-text-primary">
              {t('pages.settings.sections.systemConfig')}
            </h3>
            <div className="mt-5">
              <div className="text-center py-8 text-text-tertiary">
                {t('dashboard.sections.comingSoon')}
              </div>
            </div>
          </div>
        </div>

        <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
          <div className="px-4 py-5 sm:p-6">
            <h3 className="text-lg leading-6 font-medium text-text-primary">
              {t('pages.settings.sections.userManagement')}
            </h3>
            <div className="mt-5">
              <div className="text-center py-8 text-text-tertiary">
                {t('dashboard.sections.comingSoon')}
              </div>
            </div>
          </div>
        </div>

        <div className="bg-components-panel-bg shadow rounded-lg border border-divider-subtle">
          <div className="px-4 py-5 sm:p-6">
            <h3 className="text-lg leading-6 font-medium text-text-primary">
              {t('pages.settings.sections.networkConfig')}
            </h3>
            <div className="mt-5">
              <div className="text-center py-8 text-text-tertiary">
                {t('dashboard.sections.comingSoon')}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}