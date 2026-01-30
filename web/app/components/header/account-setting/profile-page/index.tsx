'use client'
import React, { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '@/store/provider'
import { apiPut } from '@/lib/api'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'

const ProfilePage = () => {
  const { t } = useTranslation('common')
  const { user, fetchUserProfile } = useAuthStore()
  const [isLoading, setIsLoading] = useState(false)
  const [message, setMessage] = useState('')
  const [formData, setFormData] = useState({
    name: user?.name || '',
    email: user?.email || '',
    phone: user?.phone || '',
  })
  const [passwordData, setPasswordData] = useState({
    oldPassword: '',
    newPassword: '',
    confirmPassword: '',
  })

  const handleProfileUpdate = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setMessage('')

    try {
      const response = await apiPut(`users/${user?.id}`, {
        name: formData.name,
        email: formData.email || null,
        phone: formData.phone || null,
      })

      if (String(response.code) === '0') {
        setMessage(t('userProfile.updateSuccess'))
        await fetchUserProfile()
      } else {
        setMessage((response as any).msg || t('userProfile.updateFailed'))
      }
    } catch (error) {
      setMessage(t('userProfile.updateFailedRetry'))
    } finally {
      setIsLoading(false)
    }
  }

  const handlePasswordChange = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (passwordData.newPassword !== passwordData.confirmPassword) {
      setMessage(t('userProfile.passwordMismatch'))
      return
    }

    if (passwordData.newPassword.length < 6) {
      setMessage(t('userProfile.passwordTooShort'))
      return
    }

    setIsLoading(true)
    setMessage('')

    try {
      const response = await apiPut(`users/${user?.id}/password`, {
        oldPassword: passwordData.oldPassword,
        newPassword: passwordData.newPassword,
      })

      if (String(response.code) === '0') {
        setMessage(t('userProfile.passwordChangeSuccess'))
        setPasswordData({
          oldPassword: '',
          newPassword: '',
          confirmPassword: '',
        })
      } else {
        setMessage((response as any).msg || t('userProfile.passwordChangeFailed'))
      }
    } catch (error) {
      setMessage(t('userProfile.passwordChangeFailedRetry'))
    } finally {
      setIsLoading(false)
    }
  }

  if (!user) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="text-center">
          <div className="system-md-regular text-text-tertiary">{t('userProfile.loadingUserInfo')}</div>
        </div>
      </div>
    )
  }

  return (
    <div className="max-w-[480px] space-y-8">
      {message && (
        <div className={`rounded-lg p-3 system-sm-regular ${
          message.includes('成功') 
            ? 'bg-state-success-hover text-text-success border border-state-success-solid' 
            : 'bg-state-destructive-hover text-text-destructive border border-state-destructive-border'
        }`}>
          {message}
        </div>
      )}

      {/* 个人信息 */}
      <div className="space-y-6">
        <div>
          <h3 className="system-lg-semibold text-text-primary mb-4">{t('userProfile.personalInfo')}</h3>
          <form onSubmit={handleProfileUpdate} className="space-y-6">
            <div>
              <label className="block system-sm-medium text-text-secondary mb-2">
                {t('userProfile.username')}
              </label>
              <Input
                type="text"
                value={formData.name}
                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                className="w-full"
                required
              />
            </div>
            <div>
              <label className="block system-sm-medium text-text-secondary mb-2">
                {t('userProfile.email')}
              </label>
              <Input
                type="email"
                value={formData.email}
                onChange={(e) => setFormData({ ...formData, email: e.target.value })}
                className="w-full"
                placeholder={t('userProfile.emailPlaceholder')}
              />
            </div>
            <div>
              <label className="block system-sm-medium text-text-secondary mb-2">
                {t('userProfile.phone')}
              </label>
              <Input
                type="tel"
                value={formData.phone}
                onChange={(e) => setFormData({ ...formData, phone: e.target.value })}
                className="w-full"
                placeholder={t('userProfile.phonePlaceholder')}
              />
            </div>
            <div className="pt-2">
              <Button
                type="submit"
                variant="primary"
                loading={isLoading}
                size="medium"
                className="min-w-[120px]"
              >
                {t('actions.save')}
              </Button>
            </div>
          </form>
        </div>

        {/* 分割线 */}
        <div className="border-t border-divider-subtle" />

        {/* 修改密码 */}
        <div>
          <h3 className="system-lg-semibold text-text-primary mb-4">{t('userProfile.changePassword')}</h3>
          <form onSubmit={handlePasswordChange} className="space-y-6">
            <div>
              <label className="block system-sm-medium text-text-secondary mb-2">
                {t('userProfile.currentPassword')}
              </label>
              <Input
                type="password"
                value={passwordData.oldPassword}
                onChange={(e) => setPasswordData({ ...passwordData, oldPassword: e.target.value })}
                className="w-full"
                placeholder={t('userProfile.currentPasswordPlaceholder')}
                required
              />
            </div>
            <div>
              <label className="block system-sm-medium text-text-secondary mb-2">
                {t('userProfile.newPassword')}
              </label>
              <Input
                type="password"
                value={passwordData.newPassword}
                onChange={(e) => setPasswordData({ ...passwordData, newPassword: e.target.value })}
                className="w-full"
                placeholder={t('userProfile.newPasswordPlaceholder')}
                required
                minLength={6}
              />
            </div>
            <div>
              <label className="block system-sm-medium text-text-secondary mb-2">
                {t('userProfile.confirmPassword')}
              </label>
              <Input
                type="password"
                value={passwordData.confirmPassword}
                onChange={(e) => setPasswordData({ ...passwordData, confirmPassword: e.target.value })}
                className="w-full"
                placeholder={t('userProfile.confirmPasswordPlaceholder')}
                required
                minLength={6}
              />
            </div>
            <div className="pt-2">
              <Button
                type="submit"
                variant="secondary"
                loading={isLoading}
                size="medium"
                className="min-w-[120px]"
              >
                {t('userProfile.changePassword')}
              </Button>
            </div>
          </form>
        </div>

        {/* 分割线 */}
        <div className="border-t border-divider-subtle" />

        {/* 账户信息 */}
        <div>
          <h3 className="system-lg-semibold text-text-primary mb-4">{t('userProfile.accountInfo')}</h3>
          <div className="bg-background-default-subtle border border-divider-subtle rounded-lg p-4 space-y-4">
            <div className="flex justify-between items-center">
              <span className="system-sm-medium text-text-secondary">{t('userProfile.userId')}</span>
              <span className="system-sm-regular text-text-tertiary font-mono">{user.id}</span>
            </div>
            <div className="flex justify-between items-center">
              <span className="system-sm-medium text-text-secondary">{t('userProfile.accountStatus')}</span>
              <span className={`system-xs-medium px-2 py-1 rounded-full ${
                !user.isDisabled 
                  ? 'bg-state-success-hover text-text-success border border-state-success-solid' 
                  : 'bg-state-destructive-hover text-text-destructive border border-state-destructive-border'
              }`}>
                {!user.isDisabled ? t('userProfile.statusActive') : t('userProfile.statusDisabled')}
              </span>
            </div>
            {user.dateLastLogon && (
              <div className="flex justify-between items-center">
                <span className="system-sm-medium text-text-secondary">{t('userProfile.lastLogin')}</span>
                <span className="system-sm-regular text-text-tertiary">{user.dateLastLogon}</span>
              </div>
            )}
            {user.parentId && (
              <div className="flex justify-between items-center">
                <span className="system-sm-medium text-text-secondary">{t('userProfile.parentUser')}</span>
                <span className="system-sm-regular text-text-tertiary font-mono">{user.parentId}</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

export default ProfilePage