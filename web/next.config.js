const isDev = process.env.NODE_ENV === 'development'

const withBundleAnalyzer = require('@next/bundle-analyzer')({
  enabled: process.env.ANALYZE === 'true',
})

// the default url to prevent parse url error when running jest
const hasSetWebPrefix = process.env.NEXT_PUBLIC_WEB_PREFIX
const port = process.env.PORT || 3000
const locImageURLs = !hasSetWebPrefix ? [new URL(`http://localhost:${port}/**`), new URL(`http://127.0.0.1:${port}/**`)] : []
const remoteImageURLs = [hasSetWebPrefix ? new URL(`${process.env.NEXT_PUBLIC_WEB_PREFIX}/**`) : '', ...locImageURLs].filter(item => !!item)

/** @type {import('next').NextConfig} */
const nextConfig = {
  basePath: process.env.NEXT_PUBLIC_BASE_PATH || '',
  productionBrowserSourceMaps: false, // enable browser source map generation during the production build
  // https://nextjs.org/docs/messages/next-image-unconfigured-host
  images: {
    remotePatterns: remoteImageURLs.map(remoteImageURL => ({
      protocol: remoteImageURL.protocol.replace(':', ''),
      hostname: remoteImageURL.hostname,
      port: remoteImageURL.port,
      pathname: remoteImageURL.pathname,
      search: '',
    })),
  },
  experimental: {
    optimizePackageImports: [
      '@heroicons/react'
    ],
  },
  // fix all before production. Now it slow the develop speed.
  eslint: {
    // Warning: This allows production builds to successfully complete even if
    // your project has ESLint errors.
    ignoreDuringBuilds: true,
    dirs: ['app', 'config', 'context', 'hooks', 'i18n', 'models', 'service', 'types', 'utils'],
  },
  typescript: {
    // https://nextjs.org/docs/api-reference/next.config.js/ignoring-typescript-errors
    ignoreBuildErrors: true,
  },
  reactStrictMode: true,
  async redirects() {
    return [
      {
        source: '/',
        destination: '/dashboard',
        permanent: false,
      },
    ]
  },
  async rewrites() {
    // 开发模式下代理 API 请求到后端
    if (isDev) {
      return [
        {
          source: '/api/v1/:path*',
          destination: 'http://localhost:3002/api/v1/:path*',
        },
      ]
    }
    return []
  },
  // output: 'standalone', // Disabled due to Windows symlink permission issues
  // Enable standalone output for production builds
  output: 'standalone',
  compiler: {
    removeConsole: isDev ? false : { exclude: ['warn', 'error'] },
  }
}

module.exports = withBundleAnalyzer(nextConfig)