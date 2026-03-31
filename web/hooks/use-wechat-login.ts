'use client'

import { useQuery, useMutation } from '@tanstack/react-query'
import { apiGet, apiPost } from '@/lib/api-client'
import { useAuthStore } from '@/store/provider'

interface WechatQrcodeResponse {
  qrcode_url: string
  authorize_url: string
  state: string
}

interface WechatCallbackRequest {
  code: string
  state: string
}

interface WechatLoginResponse {
  access_token: string
  token_type: string
  expires_in: number
  user_info: {
    id: string
    openid?: string
    unionid?: string
    nickname?: string
    headimgurl?: string
  }
}

export const useWechatLogin = () => {
  const { setToken, setUser } = useAuthStore()

  // 获取微信二维码
  const getQrcode = useQuery({
    queryKey: ['wechat', 'qrcode'],
    queryFn: () => apiGet<WechatQrcodeResponse>('auth/social/wechat/qrcode'),
    enabled: false, // 手动触发
  })

  // 完成微信登录（供 login page 调用）
  const completeLogin = useMutation({
    mutationFn: (data: WechatCallbackRequest) =>
      apiPost<WechatLoginResponse>('auth/social/wechat/callback', data),
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
          openid: user_info.openid,
          unionid: user_info.unionid,
          nickname: user_info.nickname,
          headimgurl: user_info.headimgurl,
        } as any)

        return response.result
      }
      throw new Error(response.msg)
    },
  })

  return {
    getQrcode,
    completeLogin,
  }
}
