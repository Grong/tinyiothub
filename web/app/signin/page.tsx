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
    <div className="flex h-screen relative overflow-hidden">
      {/* 背景渐变 */}
      <div className="absolute inset-0 -z-10 bg-gradient-to-br from-blue-50/50 via-indigo-50/30 to-purple-50/50" />

      {/* 装饰性玻璃球 */}
      <div className="absolute top-20 left-20 w-64 h-64 glass-orb opacity-60" />
      <div className="absolute bottom-32 left-40 w-48 h-48 glass-orb opacity-40" />
      <div className="absolute top-1/3 right-1/4 w-80 h-80 glass-orb opacity-30" />
      <div className="absolute bottom-20 right-32 w-56 h-56 glass-orb opacity-50" />

      {/* 左侧背景区域 */}
      <div className="hidden lg:flex lg:w-1/2 relative overflow-hidden items-center justify-center">
        {/* 深色渐变背景 */}
        <div className="absolute inset-0 brand-dark-gradient" />
        <div className="absolute inset-0 glass-section opacity-30" />

        <div className="relative z-10 flex flex-col justify-center px-16 text-white">
          <div className="max-w-lg">
            <div className="flex items-center gap-4 mb-8">
              <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-white/30 to-white/10 backdrop-blur-xl border border-white/20 flex items-center justify-center">
                <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <h1 className="text-5xl font-bold">
                TinyIoTHub
              </h1>
            </div>
            <p className="text-2xl text-white/90 mb-12 leading-relaxed">
              轻量级、高性能、企业级的物联网边缘网关系统
            </p>

            {/* 特性列表 */}
            <div className="space-y-4">
              <div className="flex items-center gap-3">
                <div className="w-8 h-8 rounded-lg bg-white/20 backdrop-blur-xl border border-white/20 flex items-center justify-center">
                  <svg className="w-4 h-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <span className="text-white/90">内置人工智能，智能驱动匹配</span>
              </div>
              <div className="flex items-center gap-3">
                <div className="w-8 h-8 rounded-lg bg-white/20 backdrop-blur-xl border border-white/20 flex items-center justify-center">
                  <svg className="w-4 h-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <span className="text-white/90">接入即自治，运行即自愈</span>
              </div>
              <div className="flex items-center gap-3">
                <div className="w-8 h-8 rounded-lg bg-white/20 backdrop-blur-xl border border-white/20 flex items-center justify-center">
                  <svg className="w-4 h-4 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <span className="text-white/90">9999+ 协议支持，开箱即用</span>
              </div>
            </div>
          </div>
        </div>

        {/* 装饰性几何图形 */}
        <div className="absolute top-24 right-24 w-40 h-40 border border-white/20 rounded-full backdrop-blur-xl"></div>
        <div className="absolute bottom-24 right-40 w-24 h-24 border border-white/20 rounded-full backdrop-blur-xl"></div>
        <div className="absolute top-1/2 right-16 w-12 h-12 bg-white/10 rounded-full backdrop-blur-xl"></div>
      </div>

      {/* 右侧登录表单 */}
      <div className="flex-1 flex items-center justify-center px-6 py-12 lg:px-8 relative">
        {/* 玻璃卡片表单容器 */}
        <div className="w-full max-w-md">
          <div className="glass rounded-3xl p-10 shadow-2xl">
            <div className="text-center mb-8">
              {/* Logo */}
              <div className="mx-auto w-16 h-16 bg-gradient-to-br from-blue-600 to-blue-700 rounded-2xl flex items-center justify-center mb-6 shadow-lg shadow-blue-600/30">
                <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <h2 className="text-3xl font-bold text-gray-900 mb-2">
                {t('auth.welcomeBack')}
              </h2>
              <p className="text-gray-600">
                {t('auth.pleaseSignIn')}
              </p>
            </div>

            <form className="space-y-6" onSubmit={handleSubmit}>
              {error && (
                <div className="glass-orange rounded-lg p-4">
                  <div className="flex">
                    <div className="flex-shrink-0">
                      <svg className="h-5 w-5 text-orange-600" viewBox="0 0 20 20" fill="currentColor">
                        <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                      </svg>
                    </div>
                    <div className="ml-3">
                      <p className="text-sm text-orange-700">{error}</p>
                    </div>
                  </div>
                </div>
              )}

              <div className="space-y-5">
                <div>
                  <label htmlFor="username" className="block text-sm font-medium text-gray-700 mb-2">
                    {t('auth.username')}
                  </label>
                  <Input
                    id="username"
                    name="username"
                    type="text"
                    required
                    className="w-full glass-input"
                    placeholder={t('auth.usernamePlaceholder')}
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                  />
                </div>

                <div>
                  <label htmlFor="password" className="block text-sm font-medium text-gray-700 mb-2">
                    {t('auth.password')}
                  </label>
                  <Input
                    id="password"
                    name="password"
                    type="password"
                    required
                    className="w-full glass-input"
                    placeholder={t('auth.passwordPlaceholder')}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </div>
              </div>

              <Button
                type="submit"
                variant="primary"
                size="large"
                loading={isLoading}
                className="w-full glass-btn-primary"
              >
                {isLoading ? t('auth.signingIn') : t('auth.signIn')}
              </Button>

              <div className="text-center">
                <p className="text-sm text-gray-500">
                  {t('auth.defaultAccount')}
                </p>
              </div>
            </form>
          </div>

          {/* 底部链接 */}
          <div className="mt-6 text-center">
            <a href="/" className="text-sm text-gray-500 hover:text-blue-600 transition-colors">
              &larr; 返回首页
            </a>
          </div>
        </div>
      </div>
    </div>
  )
}
