'use client'

import { useState, useEffect } from 'react'
import { useRouter, useSearchParams } from 'next/navigation'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '@/store/provider'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'

export default function SignInPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  
  const router = useRouter()
  const searchParams = useSearchParams()
  const { login, logout } = useAuthStore()
  const { t } = useTranslation('login')

  // 确保在登录页面清除任何旧的认证状态
  useEffect(() => {
    logout()
  }, [logout])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setIsLoading(true)
    setError('')

    try {
      await login(username, password)
      
      // 检查是否有redirect参数，如果有则跳转到指定页面，否则跳转到dashboard
      const redirectTo = searchParams.get('redirect') || '/dashboard'
      
      // 使用 window.location.href 强制页面刷新，确保认证状态正确加载
      window.location.href = redirectTo
    } catch (err) {
      setError(err instanceof Error ? err.message : t('auth.loginFailed'))
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <div className="flex h-screen">
      {/* 左侧背景区域 */}
      <div className="hidden lg:flex lg:w-1/2 bg-gradient-to-br from-primary-600 via-primary-500 to-primary-400 relative overflow-hidden">
        <div className="absolute inset-0 bg-black/10"></div>
        <div className="relative z-10 flex flex-col justify-center px-12 text-white">
          <div className="max-w-md">
            <h1 className="text-4xl font-bold mb-6">
              {t('app.title')}
            </h1>
            <p className="text-xl text-white/90 mb-8">
              {t('app.subtitle')}
            </p>
          </div>
        </div>
        {/* 装饰性几何图形 */}
        <div className="absolute top-20 right-20 w-32 h-32 border border-white/20 rounded-full"></div>
        <div className="absolute bottom-20 right-32 w-16 h-16 border border-white/20 rounded-full"></div>
        <div className="absolute top-1/2 right-12 w-8 h-8 bg-white/10 rounded-full"></div>
      </div>

      {/* 右侧登录表单 */}
      <div className="flex-1 flex items-center justify-center px-6 py-12 lg:px-8 bg-background-body">
        <div className="w-full max-w-md space-y-8">
          <div className="text-center">
            <div className="mx-auto w-16 h-16 bg-components-button-primary-bg/10 rounded-2xl flex items-center justify-center mb-6">
              <div className="w-8 h-8 bg-components-button-primary-bg rounded-lg"></div>
            </div>
            <h2 className="text-3xl font-bold text-text-primary mb-2">
              {t('auth.welcomeBack')}
            </h2>
            <p className="text-text-secondary">
              {t('auth.pleaseSignIn')}
            </p>
          </div>

          <form className="space-y-6" onSubmit={handleSubmit}>
            {error && (
              <div className="bg-state-destructive-hover border border-state-destructive-border rounded-lg p-4">
                <div className="flex">
                  <div className="flex-shrink-0">
                    <svg className="h-5 w-5 text-text-destructive" viewBox="0 0 20 20" fill="currentColor">
                      <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                    </svg>
                  </div>
                  <div className="ml-3">
                    <p className="text-sm text-text-destructive">{error}</p>
                  </div>
                </div>
              </div>
            )}

            <div className="space-y-4">
              <div>
                <label htmlFor="username" className="block text-sm font-medium text-text-secondary mb-2">
                  {t('auth.username')}
                </label>
                <Input
                  id="username"
                  name="username"
                  type="text"
                  required
                  className="w-full"
                  placeholder={t('auth.usernamePlaceholder')}
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
              </div>

              <div>
                <label htmlFor="password" className="block text-sm font-medium text-text-secondary mb-2">
                  {t('auth.password')}
                </label>
                <Input
                  id="password"
                  name="password"
                  type="password"
                  required
                  className="w-full"
                  placeholder={t('auth.passwordPlaceholder')}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </div>
            </div>

            <div>
              <Button
                type="submit"
                variant="primary"
                size="large"
                loading={isLoading}
                className="w-full"
              >
                {isLoading ? t('auth.signingIn') : t('auth.signIn')}
              </Button>
            </div>

            <div className="text-center">
              <p className="text-sm text-text-tertiary">
                {t('auth.defaultAccount')}
              </p>
            </div>
          </form>
        </div>
      </div>
    </div>
  )
}