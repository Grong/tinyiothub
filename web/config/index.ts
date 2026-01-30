export const API_PREFIX = process.env.NEXT_PUBLIC_API_PREFIX || '/api/v1'
export const PUBLIC_API_PREFIX = process.env.NEXT_PUBLIC_PUBLIC_API_PREFIX || '/api/v1'

export const IS_CE_EDITION = process.env.NEXT_PUBLIC_EDITION === 'SELF_HOSTED'

export const LOCALE_COOKIE_NAME = 'locale'
export const CSRF_COOKIE_NAME = () => 'csrftoken'
export const CSRF_HEADER_NAME = 'X-CSRFToken'

export const WEB_APP_SHARE_CODE_HEADER_NAME = 'X-Web-App-Share-Code'
export const PASSPORT_HEADER_NAME = 'X-Passport'

export const ZENDESK_WIDGET_KEY = process.env.NEXT_PUBLIC_ZENDESK_WIDGET_KEY || ''

export const basePath = process.env.NEXT_PUBLIC_BASE_PATH || ''

export const MARKETPLACE_API_PREFIX = process.env.NEXT_PUBLIC_MARKETPLACE_API_PREFIX || ''
export const ALLOW_UNSAFE_DATA_SCHEME = process.env.NEXT_PUBLIC_ALLOW_UNSAFE_DATA_SCHEME === 'true'