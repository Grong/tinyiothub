import type { NextRequest } from 'next/server'
import { NextResponse } from 'next/server'

const NECESSARY_DOMAIN = '*.sentry.io http://localhost:* http://127.0.0.1:* https://analytics.google.com googletagmanager.com *.googletagmanager.com https://www.google-analytics.com https://api.github.com'

// 需要认证的路径
const protectedPaths = [
  '/dashboard',
  '/devices',
  '/monitoring',
  '/alarms',
  '/settings',
]

// 公开路径（不需要认证）
const publicPaths = [
  '/signin',
  '/signup',
  '/forgot-password',
]

const wrapResponseWithXFrameOptions = (response: NextResponse, pathname: string) => {
  // prevent clickjacking: https://owasp.org/www-community/attacks/Clickjacking
  // Dashboard and device pages should be allowed to be embedded in iframe for monitoring
  if (process.env.NEXT_PUBLIC_ALLOW_EMBED !== 'true' && !pathname.startsWith('/dashboard') && !pathname.startsWith('/devices') && !pathname.startsWith('/monitoring'))
    response.headers.set('X-Frame-Options', 'DENY')

  return response
}

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl
  
  // 认证检查逻辑
  const token = request.cookies.get('auth-token')?.value || 
                request.headers.get('authorization')?.replace('Bearer ', '')

  // 检查是否访问受保护的路径
  const isProtectedPath = protectedPaths.some(path => pathname.startsWith(path))
  const isPublicPath = publicPaths.some(path => pathname.startsWith(path))

  // 如果访问受保护路径但没有token，重定向到登录页
  if (isProtectedPath && !token) {
    const signInUrl = new URL('/signin', request.url)
    signInUrl.searchParams.set('redirect', pathname)
    return NextResponse.redirect(signInUrl)
  }

  // 如果已登录但访问登录页，重定向到dashboard
  if (isPublicPath && token && pathname === '/signin') {
    return NextResponse.redirect(new URL('/dashboard', request.url))
  }

  // 根路径重定向
  if (pathname === '/') {
    if (token) {
      return NextResponse.redirect(new URL('/dashboard', request.url))
    } else {
      return NextResponse.redirect(new URL('/signin', request.url))
    }
  }

  const requestHeaders = new Headers(request.headers)
  const response = NextResponse.next({
    request: {
      headers: requestHeaders,
    },
  })

  const isWhiteListEnabled = !!process.env.NEXT_PUBLIC_CSP_WHITELIST && process.env.NODE_ENV === 'production'
  if (!isWhiteListEnabled)
    return wrapResponseWithXFrameOptions(response, pathname)

  const whiteList = `${process.env.NEXT_PUBLIC_CSP_WHITELIST} ${NECESSARY_DOMAIN}`
  const nonce = Buffer.from(crypto.randomUUID()).toString('base64')
  const csp = `'nonce-${nonce}'`

  const scheme_source = 'data: mediastream: blob: filesystem:'

  const cspHeader = `
    default-src 'self' ${scheme_source} ${csp} ${whiteList};
    connect-src 'self' ${scheme_source} ${csp} ${whiteList};
    script-src 'self' ${scheme_source} ${csp} ${whiteList};
    style-src 'self' 'unsafe-inline' ${scheme_source} ${whiteList};
    worker-src 'self' ${scheme_source} ${csp} ${whiteList};
    media-src 'self' ${scheme_source} ${csp} ${whiteList};
    img-src * data: blob:;
    font-src 'self';
    object-src 'none';
    base-uri 'self';
    form-action 'self';
    upgrade-insecure-requests;
`
  // Replace newline characters and spaces
  const contentSecurityPolicyHeaderValue = cspHeader
    .replace(/\s{2,}/g, ' ')
    .trim()

  requestHeaders.set('x-nonce', nonce)

  requestHeaders.set(
    'Content-Security-Policy',
    contentSecurityPolicyHeaderValue,
  )

  response.headers.set(
    'Content-Security-Policy',
    contentSecurityPolicyHeaderValue,
  )

  return wrapResponseWithXFrameOptions(response, pathname)
}

export const config = {
  matcher: [
    /*
     * Match all request paths except for the ones starting with:
     * - api (API routes)
     * - _next/static (static files)
     * - _next/image (image optimization files)
     * - favicon.ico (favicon file)
     */
    {
      source: '/((?!_next/static|_next/image|favicon.ico).*)',
    },
  ],
}