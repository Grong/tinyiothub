/**
 * 认证 API
 */

import { apiGet, apiPost } from './client.js';
import type {
  LoginRequest, LoginResponse, User,
  RegisterRequest, RegisterResponse,
  SmsSendRequest, SmsSendResponse,
  SmsLoginRequest, SmsLoginResponse,
  WechatQrcodeResponse, WechatLoginRequest, WechatLoginResponse,
} from '../types/index.js';

export const authApi = {
  async login(data: LoginRequest) {
    return apiPost<LoginResponse>('/auth/login', data);
  },

  async register(data: RegisterRequest) {
    return apiPost<RegisterResponse>('/auth/register', data);
  },

  async logout() {
    return apiPost<void>('/auth/logout');
  },

  async getCurrentUser() {
    return apiGet<User>('/users/me');
  },

  async refreshToken() {
    return apiPost<{ accessToken: string }>('/auth/refresh');
  },

  // SMS
  async smsSend(data: SmsSendRequest) {
    return apiPost<SmsSendResponse>('/auth/sms/send', data);
  },

  async smsLogin(data: SmsLoginRequest) {
    return apiPost<SmsLoginResponse>('/auth/sms/login', data);
  },

  // WeChat
  async getWechatQrcode() {
    return apiGet<WechatQrcodeResponse>('/auth/social/wechat/qrcode');
  },

  async wechatLogin(data: WechatLoginRequest) {
    return apiPost<WechatLoginResponse>('/auth/social/wechat/login', data);
  },
};
