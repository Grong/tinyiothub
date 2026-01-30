export const ACCOUNT_SETTING_TAB = {
  PROFILE: 'profile',
  MEMBERS: 'members',
  LANGUAGE: 'language',
} as const

export type AccountSettingTab = typeof ACCOUNT_SETTING_TAB[keyof typeof ACCOUNT_SETTING_TAB]