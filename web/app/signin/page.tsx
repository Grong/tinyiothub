'use client'

import { useState, useEffect, useCallback } from 'react'
import { useRouter, useSearchParams } from 'next/navigation'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '@/store/provider'
import { useSmsLogin } from '@/hooks/use-sms-login'
import { useWechatLogin } from '@/hooks/use-wechat-login'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'

type LoginMode = 'password' | 'sms' | 'wechat'

const SMS_COUNTDOWN_SECONDS = 90

export default function SignInPage() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const { login, logout } = useAuthStore()
  const { t } = useTranslation('login')
  const { sendCode, loginWithCode } = useSmsLogin()
  const { completeLogin } = useWechatLogin()

  // 登录模式
  const [loginMode, setLoginMode] = useState<LoginMode>('password')

  // 密码登录
  const [passwordForm, setPasswordForm] = useState({
    username: '',
    password: '',
  })

  // 短信登录
  const [smsForm, setSmsForm] = useState({
    phone: '',
    code: '',
  })
  const [smsCountdown, setSmsCountdown] = useState(0)
  const [smsLoading, setSmsLoading] = useState(false)
  const [smsError, setSmsError] = useState('')

  // 通用状态
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')

  // 确保在登录页面清除任何旧的认证状态
  useEffect(() => {
    logout()
  }, [logout])

  // 短信倒计时
  useEffect(() => {
    if (smsCountdown <= 0) return
    const timer = setInterval(() => {
      setSmsCountdown((prev) => prev - 1)
    }, 1000)
    return () => clearInterval(timer)
  }, [smsCountdown])

  // 监听 WeChat postMessage 回调
  useEffect(() => {
    const handleMessage = async (event: MessageEvent) => {
      if (event.origin !== window.location.origin) return

      const { type, code, state } = event.data
      if (type === 'WEIXIN_LOGIN_CALLBACK') {
        setError('')
        setIsLoading(true)
        try {
          await completeLogin.mutateAsync({ code, state })
          const redirectTo = searchParams.get('redirect') || '/dashboard'
          window.location.href = redirectTo
        } catch (err) {
          setError(err instanceof Error ? err.message : '微信登录失败')
        } finally {
          setIsLoading(false)
        }
      }
    }

    window.addEventListener('message', handleMessage)
    return () => window.removeEventListener('message', handleMessage)
  }, [completeLogin, searchParams])

  // 密码登录
  const handlePasswordChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setPasswordForm({ ...passwordForm, [e.target.name]: e.target.value })
  }

  const handlePasswordSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    setIsLoading(true)

    try {
      await login(passwordForm.username, passwordForm.password)
      const redirectTo = searchParams.get('redirect') || '/dashboard'
      window.location.href = redirectTo
    } catch (err) {
      setError(err instanceof Error ? err.message : t('auth.loginFailed'))
    } finally {
      setIsLoading(false)
    }
  }

  // 短信登录
  const handleSmsChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setSmsForm({ ...smsForm, [e.target.name]: e.target.value })
  }

  const handleSendSms = async () => {
    if (!smsForm.phone) {
      setSmsError('请输入手机号')
      return
    }
    if (!/^1[3-9]\d{9}$/.test(smsForm.phone)) {
      setSmsError('手机号格式不正确')
      return
    }
    setSmsError('')
    setSmsLoading(true)
    try {
      await sendCode.mutateAsync({ phone: smsForm.phone })
      setSmsCountdown(SMS_COUNTDOWN_SECONDS)
    } catch (err) {
      setSmsError(err instanceof Error ? err.message : '发送验证码失败')
    } finally {
      setSmsLoading(false)
    }
  }

  const handleSmsSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    if (!smsForm.phone) {
      setError('请输入手机号')
      return
    }
    if (!smsForm.code) {
      setError('请输入验证码')
      return
    }
    setIsLoading(true)
    try {
      await loginWithCode.mutateAsync({ phone: smsForm.phone, code: smsForm.code })
      const redirectTo = searchParams.get('redirect') || '/dashboard'
      window.location.href = redirectTo
    } catch (err) {
      setError(err instanceof Error ? err.message : '登录失败，请稍后重试')
    } finally {
      setIsLoading(false)
    }
  }

  // 微信登录
  const handleWechatLogin = useCallback(() => {
    const width = 600
    const height = 700
    const left = window.screenX + (window.outerWidth - width) / 2
    const top = window.screenY + (window.outerHeight - height) / 2
    window.open(
      '/api/v1/auth/social/wechat/qrcode',
      'wechat_login',
      `width=${width},height=${height},left=${left},top=${top},scrollbars=no,resizable=no`
    )
  }, [])

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
            <div className="text-center mb-6">
              {/* Logo */}
              <div className="mx-auto w-16 h-16 bg-gradient-to-br from-blue-600 to-blue-700 rounded-2xl flex items-center justify-center mb-6 shadow-lg shadow-blue-600/30">
                <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <h2 className="text-2xl font-bold text-gray-900 mb-1">
                {t('auth.welcomeBack')}
              </h2>
              <p className="text-gray-600 text-sm">
                欢迎回来
              </p>
            </div>

            {/* 登录模式切换 */}
            <div className="flex mb-6 border border-gray-200 rounded-lg overflow-hidden">
              <button
                type="button"
                onClick={() => { setLoginMode('password'); setError('') }}
                className={`flex-1 py-2 text-sm font-medium transition-colors ${
                  loginMode === 'password'
                    ? 'bg-primary-600 text-white'
                    : 'bg-white text-gray-700 hover:bg-gray-50'
                }`}
              >
                密码登录
              </button>
              <button
                type="button"
                onClick={() => { setLoginMode('sms'); setError('') }}
                className={`flex-1 py-2 text-sm font-medium transition-colors ${
                  loginMode === 'sms'
                    ? 'bg-primary-600 text-white'
                    : 'bg-white text-gray-700 hover:bg-gray-50'
                }`}
              >
                短信登录
              </button>
              <button
                type="button"
                onClick={() => { setLoginMode('wechat'); setError('') }}
                className={`flex-1 py-2 text-sm font-medium transition-colors ${
                  loginMode === 'wechat'
                    ? 'bg-green-600 text-white'
                    : 'bg-white text-gray-700 hover:bg-gray-50'
                }`}
              >
                微信登录
              </button>
            </div>

            {/* 错误提示 */}
            {error && (
              <div className="mb-4 p-3 bg-red-50 border border-red-200 text-red-600 rounded-lg text-sm">
                {error}
              </div>
            )}

            {/* 密码登录表单 */}
            {loginMode === 'password' && (
              <form onSubmit={handlePasswordSubmit} className="space-y-5">
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
                    value={passwordForm.username}
                    onChange={handlePasswordChange}
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
                    value={passwordForm.password}
                    onChange={handlePasswordChange}
                  />
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
            )}

            {/* 短信登录表单 */}
            {loginMode === 'sms' && (
              <form onSubmit={handleSmsSubmit} className="space-y-5">
                <div>
                  <label htmlFor="sms-phone" className="block text-sm font-medium text-gray-700 mb-2">
                    手机号
                  </label>
                  <Input
                    id="sms-phone"
                    name="phone"
                    type="tel"
                    required
                    className="w-full glass-input"
                    placeholder="请输入手机号"
                    value={smsForm.phone}
                    onChange={handleSmsChange}
                  />
                </div>

                <div>
                  <label htmlFor="sms-code" className="block text-sm font-medium text-gray-700 mb-2">
                    验证码
                  </label>
                  <div className="flex gap-2">
                    <Input
                      id="sms-code"
                      name="code"
                      type="text"
                      required
                      maxLength={6}
                      className="flex-1 glass-input"
                      placeholder="请输入验证码"
                      value={smsForm.code}
                      onChange={handleSmsChange}
                    />
                    <button
                      type="button"
                      onClick={handleSendSms}
                      disabled={smsLoading || smsCountdown > 0}
                      className="px-4 py-2 text-sm font-medium text-primary-600 border border-primary-600 rounded-lg hover:bg-primary-50 disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                    >
                      {smsCountdown > 0 ? `${smsCountdown}s` : '获取验证码'}
                    </button>
                  </div>
                  {smsError && (
                    <p className="mt-1 text-sm text-red-600">{smsError}</p>
                  )}
                </div>

                <Button
                  type="submit"
                  variant="primary"
                  size="large"
                  loading={isLoading}
                  className="w-full glass-btn-primary"
                >
                  {isLoading ? '登录中...' : '登录'}
                </Button>
              </form>
            )}

            {/* 微信登录 */}
            {loginMode === 'wechat' && (
              <div className="space-y-5">
                <p className="text-center text-sm text-gray-600">
                  点击下方按钮，使用微信扫码登录
                </p>
                <button
                  type="button"
                  onClick={handleWechatLogin}
                  disabled={isLoading}
                  className="w-full py-3 px-4 bg-green-600 hover:bg-green-700 text-white font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
                >
                  <svg className="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M8.691 2.188C3.891 2.188 0 5.476 0 9.53c0 2.212 1.17 4.203 3.002 5.55a.59.59 0 0 1 .213.665l-.39 1.48c-.019.07-.048.141-.048.213 0 .163.13.295.29.295a.326.326 0 0 0 .167-.054l1.903-1.114a.864.864 0 0 1 .717-.098 10.16 10.16 0 0 0 2.837.403c.276 0 .543-.027.811-.05-.857-2.578.157-4.972 1.932-6.446 1.703-1.415 3.882-1.98 5.853-1.838-.576-3.583-4.196-6.348-8.596-6.348zM5.785 5.991c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 0 1-1.162 1.178A1.17 1.17 0 0 1 4.623 7.17c0-.651.52-1.18 1.162-1.18zm5.813 0c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 0 1-1.162 1.178 1.17 1.17 0 0 1-1.162-1.178c0-.651.52-1.18 1.162-1.18zm3.348 3.86c-1.352-.052-2.559-.272-3.651-.631a.722.722 0 0 1-.537-.835.703.703 0 0 1 .827-.536c1.163.38 2.406.603 3.71.664.088.002.174.017.258.045l1.678.523a.243.243 0 0 0 .262-.082.236.236 0 0 0 .093-.25l-.178-1.68a.227.227 0 0 0-.07-.144.238.238 0 0 0-.154-.06 9.21 9.21 0 0 1-2.238.313c-1.293 0-2.615-.23-3.786-.674a.664.664 0 0 1-.43-.79.68.68 0 0 1 .8-.431c1.23.465 2.584.7 3.901.7.51 0 1.02-.043 1.53-.12a.747.747 0 0 1 .726.39l.656 1.394a.787.787 0 0 1-.127.884 8.17 8.17 0 0 1-2.37 1.54.75.75 0 0 1-.858-.174l-1.022-1.1a.242.242 0 0 0-.17-.065.238.238 0 0 0-.167.067zm-5.18-1.83a.968.968 0 0 1-.96.97.96.96 0 1 1 0-1.92.968.968 0 0 1 .96.95zm5.122 0a.968.968 0 0 1-.96.97.96.96 0 1 1 0-1.92.968.968 0 0 1 .96.95z"/>
                  </svg>
                  微信扫码登录
                </button>
                <p className="text-center text-xs text-gray-400">
                  登录成功后窗口将自动关闭
                </p>
              </div>
            )}
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
