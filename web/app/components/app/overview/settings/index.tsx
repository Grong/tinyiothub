'use client'
import type { FC } from 'react'
import React, { useEffect, useState } from 'react'
import { RiArrowRightSLine, RiCloseLine } from '@remixicon/react'
import Link from 'next/link'
import { Trans, useTranslation } from 'react-i18next'
import { useContext } from 'use-context-selector'
import Modal from '@/app/components/base/modal'
import ActionButton from '@/app/components/base/action-button'
import Button from '@/app/components/base/button'
import Divider from '@/app/components/base/divider'
import Input from '@/app/components/base/input'
import Textarea from '@/app/components/base/textarea'
import AppIcon from '@/app/components/base/app-icon'
import { SimpleSelect } from '@/app/components/base/select'
import type { AppDetailResponse } from '@/models/app'
import type { AppIconType, AppSSO, Language } from '@/types/app'
import { useToastContext } from '@/app/components/base/toast'
import { LanguagesSupported, languages } from '@/i18n/language'
import type { AppIconSelection } from '@/app/components/base/app-icon-picker'
import AppIconPicker from '@/app/components/base/app-icon-picker'
import I18n from '@/context/i18n'
import cn from '@/utils/classnames'

export type ISettingsModalProps = {
  appInfo: AppDetailResponse & Partial<AppSSO>
  isShow: boolean
  defaultValue?: string
  onClose: () => void
  onSave?: (params: ConfigParams) => Promise<void>
}

export type ConfigParams = {
  title: string
  description: string
  default_language: string
  chat_color_theme: string
  chat_color_theme_inverted: boolean
  prompt_public: boolean
  copyright: string
  privacy_policy: string
  custom_disclaimer: string
  icon_type: AppIconType
  icon: string
  icon_background?: string
  use_icon_as_answer_icon: boolean
  enable_sso?: boolean
}

const prefixSettings = 'appOverview.overview.appInfo.settings'

const SettingsModal: FC<ISettingsModalProps> = ({
  appInfo,
  isShow = false,
  onClose,
  onSave,
}) => {
  const { notify } = useToastContext()
  const [isShowMore, setIsShowMore] = useState(false)
  const {
    title,
    icon_type,
    icon,
    icon_background,
    icon_url,
    description,
    chat_color_theme,
    chat_color_theme_inverted,
    copyright,
    privacy_policy,
    custom_disclaimer,
    default_language,
    show_workflow_steps,
    use_icon_as_answer_icon,
  } = appInfo.site || {}
  const [inputInfo, setInputInfo] = useState({
    title,
    desc: description,
    chatColorTheme: chat_color_theme,
    chatColorThemeInverted: chat_color_theme_inverted,
    copyright,
    copyrightSwitchValue: !!copyright,
    privacyPolicy: privacy_policy,
    customDisclaimer: custom_disclaimer,
    use_icon_as_answer_icon,
    enable_sso: appInfo.enable_sso,
  })
  const [language, setLanguage] = useState(default_language)
  const [saveLoading, setSaveLoading] = useState(false)
  const { t } = useTranslation()
  const { locale } = useContext(I18n)

  const [showAppIconPicker, setShowAppIconPicker] = useState(false)
  const [appIcon, setAppIcon] = useState<AppIconSelection>(
    icon_type === 'image'
      ? { type: 'image', url: icon_url!, fileId: icon }
      : { type: 'emoji', icon, background: icon_background! },
  )

  useEffect(() => {
    setInputInfo({
      title,
      desc: description,
      chatColorTheme: chat_color_theme,
      chatColorThemeInverted: chat_color_theme_inverted,
      copyright,
      copyrightSwitchValue: !!copyright,
      privacyPolicy: privacy_policy,
      customDisclaimer: custom_disclaimer,
      use_icon_as_answer_icon,
      enable_sso: appInfo.enable_sso,
    })
    setLanguage(default_language)
    setAppIcon(icon_type === 'image'
      ? { type: 'image', url: icon_url!, fileId: icon }
      : { type: 'emoji', icon, background: icon_background! })
  }, [appInfo, chat_color_theme, chat_color_theme_inverted, copyright, custom_disclaimer, default_language, description, icon, icon_background, icon_type, icon_url, privacy_policy, show_workflow_steps, title, use_icon_as_answer_icon])

  const onHide = () => {
    onClose()
    setTimeout(() => {
      setIsShowMore(false)
    }, 200)
  }

  const onClickSave = async () => {
    if (!inputInfo.title) {
      notify({ type: 'error', message: t('app.newApp.nameNotEmpty') })
      return
    }

    const validateColorHex = (hex: string | null) => {
      if (hex === null || hex?.length === 0)
        return true

      const regex = /#([A-Fa-f0-9]{6})/
      const check = regex.test(hex)
      return check
    }

    const validatePrivacyPolicy = (privacyPolicy: string | null) => {
      if (privacyPolicy === null || privacyPolicy?.length === 0)
        return true

      return privacyPolicy.startsWith('http://') || privacyPolicy.startsWith('https://')
    }

    if (inputInfo !== null) {
      if (!validateColorHex(inputInfo.chatColorTheme)) {
        notify({ type: 'error', message: t(`${prefixSettings}.invalidHexMessage`) })
        return
      }
      if (!validatePrivacyPolicy(inputInfo.privacyPolicy)) {
        notify({ type: 'error', message: t(`${prefixSettings}.invalidPrivacyPolicy`) })
        return
      }
    }

    setSaveLoading(true)
    const params = {
      title: inputInfo.title,
      description: inputInfo.desc,
      default_language: language,
      chat_color_theme: inputInfo.chatColorTheme,
      chat_color_theme_inverted: inputInfo.chatColorThemeInverted,
      prompt_public: false,
      copyright: inputInfo.copyrightSwitchValue
        ? inputInfo.copyright
        : '',
      privacy_policy: inputInfo.privacyPolicy,
      custom_disclaimer: inputInfo.customDisclaimer,
      icon_type: appIcon.type,
      icon: appIcon.type === 'emoji' ? appIcon.icon : appIcon.fileId,
      icon_background: appIcon.type === 'emoji' ? appIcon.background : undefined,
      use_icon_as_answer_icon: inputInfo.use_icon_as_answer_icon,
      enable_sso: inputInfo.enable_sso,
    }
    await onSave?.(params)
    setSaveLoading(false)
    onHide()
  }

  const onChange = (field: string) => {
    return (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      let value: string | boolean
      if (e.target.type === 'checkbox')
        value = (e.target as HTMLInputElement).checked
      else
        value = e.target.value

      setInputInfo(item => ({ ...item, [field]: value }))
    }
  }

  const onDesChange = (value: string) => {
    setInputInfo(item => ({ ...item, desc: value }))
  }

  return (
    <>
      <Modal
        isShow={isShow}
        closable={false}
        onClose={onHide}
        className='max-w-[520px] p-0'
      >
        {/* header */}
        <div className='pb-3 pl-6 pr-5 pt-5'>
          <div className='flex items-center gap-1'>
            <div className='title-2xl-semi-bold grow text-text-primary'>{t(`${prefixSettings}.title`)}</div>
            <ActionButton className='shrink-0' onClick={onHide}>
              <RiCloseLine className='h-4 w-4' />
            </ActionButton>
          </div>
          <div className='system-xs-regular mt-0.5 text-text-tertiary'>
            <span>{t(`${prefixSettings}.modalTip`)}</span>
            <Link href={`${locale === LanguagesSupported[1] ? 'https://docs.tinyiothub.com/zh-hans' : 'https://docs.tinyiothub.com/en'}`} target='_blank' rel='noopener noreferrer' className='text-text-accent'>{t('common.operation.learnMore')}</Link>
          </div>
        </div>
        {/* form body */}
        <div className='space-y-5 px-6 py-3'>
          {/* name & icon */}
          <div className='flex gap-4'>
            <div className='grow'>
              <div className={cn('system-sm-semibold mb-1 py-1 text-text-secondary')}>{t(`${prefixSettings}.webName`)}</div>
              <Input
                className='w-full'
                value={inputInfo.title}
                onChange={onChange('title')}
                placeholder={t('app.appNamePlaceholder') || ''}
              />
            </div>
            <AppIcon
              size='xxl'
              onClick={() => { setShowAppIconPicker(true) }}
              className='mt-2 cursor-pointer'
              iconType={appIcon.type}
              icon={appIcon.type === 'image' ? appIcon.fileId : appIcon.icon}
              background={appIcon.type === 'image' ? undefined : appIcon.background}
              imageUrl={appIcon.type === 'image' ? appIcon.url : undefined}
            />
          </div>
          {/* description */}
          <div className='relative'>
            <div className={cn('system-sm-semibold py-1 text-text-secondary')}>{t(`${prefixSettings}.webDesc`)}</div>
            <Textarea
              className='mt-1'
              value={inputInfo.desc}
              onChange={e => onDesChange(e.target.value)}
              placeholder={t(`${prefixSettings}.webDescPlaceholder`) as string}
            />
            <p className={cn('body-xs-regular pb-0.5 text-text-tertiary')}>{t(`${prefixSettings}.webDescTip`)}</p>
          </div>
          <Divider className="my-0 h-px" />
          {/* language */}
          <div className='flex items-center'>
            <div className={cn('system-sm-semibold grow py-1 text-text-secondary')}>{t(`${prefixSettings}.language`)}</div>
            <SimpleSelect
              wrapperClassName='w-[200px]'
              items={languages.filter(item => item.supported)}
              defaultValue={language}
              onSelect={item => setLanguage(item.value as Language)}
              notClearable
            />
          </div>
          {/* more settings switch */}
          <Divider className="my-0 h-px" />
          {!isShowMore && (
            <div className='flex cursor-pointer items-center' onClick={() => setIsShowMore(true)}>
              <div className='grow'>
                <div className={cn('system-sm-semibold py-1 text-text-secondary')}>{t(`${prefixSettings}.more.entry`)}</div>
                <p className={cn('body-xs-regular pb-0.5 text-text-tertiary')}>{t(`${prefixSettings}.more.copyRightPlaceholder`)} & {t(`${prefixSettings}.more.privacyPolicyPlaceholder`)}</p>
              </div>
              <RiArrowRightSLine className='ml-1 h-4 w-4 shrink-0 text-text-secondary' />
            </div>
          )}
          {/* more settings */}
          {isShowMore && (
            <>
              {/* copyright */}
              <div className='w-full'>
                <div className='flex items-center'>
                  <div className='flex grow items-center'>
                    <div className={cn('system-sm-semibold mr-1 py-1 text-text-secondary')}>{t(`${prefixSettings}.more.copyright`)}</div>
                  </div>
                </div>
                <p className='body-xs-regular pb-0.5 text-text-tertiary'>{t(`${prefixSettings}.more.copyrightTip`)}</p>
                {inputInfo.copyrightSwitchValue && (
                  <Input
                    className='mt-2 h-10'
                    value={inputInfo.copyright}
                    onChange={onChange('copyright')}
                    placeholder={t(`${prefixSettings}.more.copyRightPlaceholder`) as string}
                  />
                )}
              </div>
              {/* privacy policy */}
              <div className='w-full'>
                <div className={cn('system-sm-semibold py-1 text-text-secondary')}>{t(`${prefixSettings}.more.privacyPolicy`)}</div>
                <p className={cn('body-xs-regular pb-0.5 text-text-tertiary')}>
                  <Trans
                    i18nKey={`${prefixSettings}.more.privacyPolicyTip`}
                    components={{ privacyPolicyLink: <Link href={'https://tinyiothub.com/privacy'} target='_blank' rel='noopener noreferrer' className='text-text-accent' /> }}
                  />
                </p>
                <Input
                  className='mt-1'
                  value={inputInfo.privacyPolicy}
                  onChange={onChange('privacyPolicy')}
                  placeholder={t(`${prefixSettings}.more.privacyPolicyPlaceholder`) as string}
                />
              </div>
              {/* custom disclaimer */}
              <div className='w-full'>
                <div className={cn('system-sm-semibold py-1 text-text-secondary')}>{t(`${prefixSettings}.more.customDisclaimer`)}</div>
                <p className={cn('body-xs-regular pb-0.5 text-text-tertiary')}>{t(`${prefixSettings}.more.customDisclaimerTip`)}</p>
                <Textarea
                  className='mt-1'
                  value={inputInfo.customDisclaimer}
                  onChange={onChange('customDisclaimer')}
                  placeholder={t(`${prefixSettings}.more.customDisclaimerPlaceholder`) as string}
                />
              </div>
            </>
          )}
        </div>
        {/* footer */}
        <div className='flex justify-end p-6 pt-5'>
          <Button className='mr-2' onClick={onHide}>{t('common.operation.cancel')}</Button>
          <Button variant='primary' onClick={onClickSave} loading={saveLoading}>{t('common.operation.save')}</Button>
        </div>
        {showAppIconPicker && (
          <div onClick={e => e.stopPropagation()}>
            <AppIconPicker
              onSelect={(payload) => {
                setAppIcon(payload)
                setShowAppIconPicker(false)
              }}
              onClose={() => {
                setAppIcon(icon_type === 'image'
                  ? { type: 'image', url: icon_url!, fileId: icon }
                  : { type: 'emoji', icon, background: icon_background! })
                setShowAppIconPicker(false)
              }}
            />
          </div>
        )}
      </Modal>
    </>
  )
}
export default React.memo(SettingsModal)
