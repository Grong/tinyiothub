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
  expires_in: number
  message: string
}

interface LoginResponse {
  access_token: string
  token_type: string
  expires_in: number
  user_info: {
    id: string
    phone: string
    username?: string
    display_name?: string
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
        const { access_token, user_info } = response.result

        // Store token in sessionStorage
        if (typeof window !== 'undefined') {
          sessionStorage.setItem('auth-token', access_token)
        }

        // Set auth state
        setToken(access_token)
        setUser({
          id: user_info.id,
          phone: user_info.phone,
          username: user_info.username,
          display_name: user_info.display_name,
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
