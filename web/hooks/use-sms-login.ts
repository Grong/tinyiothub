'use client'

import { useMutation } from '@tanstack/react-query'
import { apiPost } from '@/lib/api-client'
import { useAuthStore } from '@/store/provider'

interface SendSmsCodeRequest {
  phone: string
  captcha_ticket?: string
  captcha_randstr?: string
}

interface SmsLoginRequest {
  phone: string
  code: string
}

interface SmsCodeResponse {
  expiresIn: number
  message: string
}

interface LoginResponse {
  accessToken: string
  tokenType: string
  expiresIn: number
  userInfo: {
    id: string
    phone: string
    username?: string
    displayName?: string
  }
}

export const useSmsLogin = () => {
  const { setToken, setUser } = useAuthStore()

  const sendCode = useMutation({
    mutationFn: (data: SendSmsCodeRequest) =>
      apiPost<SmsCodeResponse>('auth/sms/send', data),
  })

  const loginWithCode = useMutation({
    mutationFn: (data: SmsLoginRequest) =>
      apiPost<LoginResponse>('auth/sms/login', data),
    onSuccess: async (response) => {
      if (response.code === 0 && response.result) {
        const { accessToken, userInfo } = response.result

        // Store token in sessionStorage
        if (typeof window !== 'undefined') {
          sessionStorage.setItem('auth-token', accessToken)
        }

        // Set auth state
        setToken(accessToken)
        setUser({
          id: userInfo.id,
          phone: userInfo.phone,
          username: userInfo.username,
          displayName: userInfo.displayName,
        } as any)

        return response.result
      }
      throw new Error(response.msg)
    },
  })

  return {
    sendCode,
    loginWithCode,
  }
}
