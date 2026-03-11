/**
 * Next.js 配置
 * 生产环境直接输出到 out 目录，由 Rust 后端服务
 */

const isDev = process.env.NODE_ENV === 'development'

const withBundleAnalyzer = require('@next/bundle-analyzer')({
  enabled: process.env.ANALYZE === 'true',
})

const port = process.env.PORT || 3000
const hasSetWebPrefix = process.env.NEXT_PUBLIC_WEB_PREFIX
const locImageURLs = !hasSetWebPrefix ? [new URL(`http://localhost:${port}/**`), new URL(`http://127.0.0.1:${port}/**`)] : []
const remoteImageURLs = [hasSetWebPrefix ? new URL(`${process.env.NEXT_PUBLIC_WEB_PREFIX}/**`) : '', ...locImageURLs].filter(item => !!item)

/** @type {import('next').NextConfig} */
const nextConfig = {
  basePath: process.env.NEXT_PUBLIC_BASE_PATH || '',
  productionBrowserSourceMaps: false,
  
  // 静态导出
  output: 'export',
  distDir: 'out',
  
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
  
  eslint: {
    ignoreDuringBuilds: true,
    dirs: ['app', 'config', 'context', 'hooks', 'i18n', 'models', 'service', 'types', 'utils'],
  },
  
  typescript: {
    ignoreBuildErrors: true,
  },
  
  reactStrictMode: true,
  
  compiler: {
    removeConsole: isDev ? false : { exclude: ['warn', 'error'] },
  }
}

module.exports = withBundleAnalyzer(nextConfig)