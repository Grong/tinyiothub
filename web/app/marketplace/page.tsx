'use client'

import React from 'react'
import TemplateMarketplace from '../components/marketplace/template-marketplace'
import DriverMarketplace from '../components/marketplace/driver-marketplace'
import { useEffect } from 'react'
import { ArrowRightIcon } from '@heroicons/react/24/outline'

export default function MarketplacePage() {
  useEffect(() => {
    document.title = '市场 | tinyiothub'
  }, [])

  return (
    <div style={{ minHeight: '100vh', overflowY: 'auto' }}>
      {/* Background */}
      <div className="fixed inset-0 -z-10 bg-gradient-to-br from-slate-50 via-blue-50/40 to-indigo-50/60" />

      {/* Navigation - same as homepage */}
      <nav className="sticky top-0 z-50 bg-white/70 backdrop-blur-xl border-b border-white/30 shadow-sm">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-8">
              <a href="/" className="flex items-center gap-2 group">
                <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-blue-600 to-blue-700 text-white shadow-lg shadow-blue-600/30 transition-transform group-hover:scale-105">
                  <ArrowRightIcon className="h-5 w-5" />
                </div>
                <span className="text-xl font-bold text-gray-900">TinyIoTHub</span>
              </a>
              <div className="hidden lg:flex items-center gap-8">
                <a href="/marketplace" className="text-sm font-medium text-blue-600 hover:text-blue-700 transition-colors">市场</a>
                <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" className="text-sm font-medium text-gray-600 hover:text-blue-600 transition-colors">文档</a>
                <a href="#" className="text-sm font-medium text-gray-600 hover:text-blue-600 transition-colors">解决方案</a>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <a href="https://github.com/Grong/tinyiothub" target="_blank" rel="noopener noreferrer" className="text-gray-400 hover:text-gray-700 transition-colors">
                <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 24 24">
                  <path fillRule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" clipRule="evenodd" />
                </svg>
              </a>
              <a href="/signin" className="text-sm font-medium text-gray-600 hover:text-blue-600 transition-colors">登录</a>
              <a href="/signin" className="rounded-lg bg-blue-600 px-5 py-2.5 text-sm font-semibold text-white shadow-lg shadow-blue-600/30 hover:bg-blue-700 transition-all">免费试用</a>
            </div>
          </div>
        </div>
      </nav>

      {/* Content */}
      <div className="px-12 py-8">
        {/* 设备模板市场 */}
        <div className="mb-8">
          <div className="glass rounded-2xl p-6 mb-4">
            <h2 className="text-lg font-semibold text-gray-900">
              设备模板
            </h2>
            <p className="text-sm text-gray-500 mt-1">从市场安装设备模板，快速接入设备</p>
          </div>
          <TemplateMarketplace />
        </div>

        {/* 驱动程序市场 */}
        <div className="mb-8">
          <div className="glass rounded-2xl p-6 mb-4">
            <h2 className="text-lg font-semibold text-gray-900">
              驱动程序
            </h2>
            <p className="text-sm text-gray-500 mt-1">从市场安装驱动程序，支持多种协议</p>
          </div>
          <DriverMarketplace />
        </div>
      </div>
    </div>
  )
}