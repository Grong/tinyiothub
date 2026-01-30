// Next.js Configuration for Static Export
// No Node.js runtime required

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'export',
  
  // 禁用图片优化
  images: {
    unoptimized: true,
  },
  
  basePath: '',
  
  // 生产环境配置
  productionBrowserSourceMaps: false,
  reactStrictMode: true,
  
  compiler: {
    removeConsole: { exclude: ['warn', 'error'] },
  },
  
  // 完全跳过 ESLint 和 TypeScript 检查
  eslint: {
    ignoreDuringBuilds: true,
  },
  typescript: {
    ignoreBuildErrors: true,
  },
  
  // 优化包导入
  experimental: {
    optimizePackageImports: ['@heroicons/react'],
  },
  
  trailingSlash: true,
}

module.exports = nextConfig
