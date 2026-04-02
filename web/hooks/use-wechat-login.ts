'use client'

import { useQuery } from '@tanstack/react-query'
import { apiGet } from '@/lib/api-client'
import { useAuthStore } from '@/store/provider'

interface WechatQrcodeResponse {
  qrcode_url: string
  authorize_url: string
  state: string
}

interface WechatLoginResult {
  access_token: string
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

  // 完成微信登录（由 login page 通过 postMessage 调用，token 直接传入）
  const completeLogin = async (accessToken: string, userInfo?: WechatLoginResult['user_info']) => {
    // Store token in sessionStorage
    if (typeof window !== 'undefined') {
      sessionStorage.setItem('auth-token', accessToken)
    }

    // Set auth state
    setToken(accessToken)
    if (userInfo) {
      setUser({
        id: userInfo.id,
        openid: userInfo.openid,
        unionid: userInfo.unionid,
        nickname: userInfo.nickname,
        headimgurl: userInfo.headimgurl,
      } as any)
    }

    return { access_token: accessToken }
  }

  return {
    getQrcode,
    completeLogin,
  }
}
