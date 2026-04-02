'use client'

import { ArrowRightIcon, CubeIcon, DevicePhoneMobileIcon, LockClosedIcon, CloudIcon, CpuChipIcon, SparklesIcon, ShieldCheckIcon, BoltIcon, RadioIcon, CommandLineIcon, ArrowPathIcon, LanguageIcon } from '@heroicons/react/24/outline'
import { useState, useEffect } from 'react'
import clsx from 'clsx'
import { basePath } from '@/utils/var'

const protocols = [
  { name: 'Modbus', desc: 'RTU/TCP' },
  { name: 'ONVIF', desc: '摄像头' },
  { name: 'SNMP', desc: '网络设备' },
  { name: 'MQTT', desc: '消息推送' },
]

const stats = [
  { value: '99.99%', label: '服务可用性' },
  { value: '9999+', label: '协议支持' },
  { value: '<50ms', label: '采集延迟' },
  { value: '7*24', label: '全天候监控' },
]

const coreFeatures = [
  { icon: SparklesIcon, title: '接入即自治', desc: '自然语言描述设备，自动完成驱动匹配与生成', color: 'blue' },
  { icon: ArrowPathIcon, title: '运行即自愈', desc: '分级自愈机制，主动发现并修复故障', color: 'green' },
  { icon: RadioIcon, title: 'LoRa无线化', desc: '免布线施工，改造无需停产', color: 'purple' },
  { icon: LanguageIcon, title: '持续进化', desc: '云端驱动库与知识库不断积累', color: 'orange' },
]

const agentFeatures = [
  {
    icon: CommandLineIcon,
    title: '自然语言交互',
    description: '用日常语言配置设备、查询状态，无需专业背景',
    color: 'blue',
  },
  {
    icon: CpuChipIcon,
    title: '智能驱动匹配',
    description: 'AI自动匹配驱动库，无匹配则自动生成并测试验证',
    color: 'green',
  },
  {
    icon: ShieldCheckIcon,
    title: '分级自愈机制',
    description: 'L0-L3分级处理，从被动响应到主动运维',
    color: 'purple',
  },
  {
    icon: BoltIcon,
    title: '心跳探针',
    description: '定期自检网关与子设备，提前发现隐患',
    color: 'orange',
  },
  {
    icon: CloudIcon,
    title: '云端协同',
    description: '状态上报、工单联动、知识闭环',
    color: 'red',
  },
  {
    icon: DevicePhoneMobileIcon,
    title: 'LoRa无线接入',
    description: '内置LoRa网关，远距离低功耗免布线',
    color: 'indigo',
  },
]

const scenarios = [
  { title: '智慧工厂', desc: '老旧设备数字化改造，分钟级接入，零布线' },
  { title: '智慧楼宇', desc: '多系统统一接入，自然语言运维' },
  { title: '分布式能源', desc: '边缘自治调度，断网不断服' },
]

export default function Home() {
  const [isNavVisible, setIsNavVisible] = useState(true)

  useEffect(() => {
    document.title = 'tinyiothub | 物联网智能平台'
  }, [])

  // 滚动隐藏/显示导航
  useEffect(() => {
    let lastScrollY = 0

    const handleScroll = () => {
      const currentScrollY = window.scrollY
      const threshold = 80

      if (currentScrollY > lastScrollY && currentScrollY > threshold) {
        setIsNavVisible(false)
      } else {
        setIsNavVisible(true)
      }

      lastScrollY = currentScrollY
    }

    window.addEventListener('scroll', handleScroll, { passive: true })
    return () => window.removeEventListener('scroll', handleScroll)
  }, [])

  return (
    <div>
      {/* Background */}
      <div className="fixed inset-0 -z-10 bg-gradient-to-br from-slate-50 via-blue-50/40 to-indigo-50/60 dark:from-slate-950 dark:via-slate-900/80 dark:to-indigo-950/60" />

      {/* Navigation */}
      <nav className={`fixed inset-x-0 top-0 z-50 glass-nav border-b border-white/30 dark:border-white/10 transition-transform duration-300 ease-out ${isNavVisible ? 'translate-y-0' : '-translate-y-full'}`}>
        <div className="px-4 md:px-6 lg:px-8">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-8">
              <a href="/" className="flex items-center gap-2 group">
                <img src={`${basePath}/logo.svg`} alt="logo" className="h-9 w-9 object-contain homepage-logo" />
                <span className="text-xl font-bold text-primary homepage-nav-text">TinyIoTHub</span>
              </a>
              <div className="hidden lg:flex items-center gap-1">
                <a href="/dashboard" className="flex h-8 items-center rounded-xl px-3 text-sm font-medium text-components-main-nav-nav-button-text hover:bg-components-main-nav-nav-button-bg-hover transition-colors homepage-nav-text">仪表盘</a>
                <a href="/marketplace" className="flex h-8 items-center rounded-xl px-3 text-sm font-medium text-components-main-nav-nav-button-text hover:bg-components-main-nav-nav-button-bg-hover transition-colors homepage-nav-text">市场</a>
                <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" className="flex h-8 items-center rounded-xl px-3 text-sm font-medium text-components-main-nav-nav-button-text hover:bg-components-main-nav-nav-button-bg-hover transition-colors homepage-nav-text">文档</a>
              </div>
            </div>
            <div className="flex items-center gap-4">
              <a href="https://github.com/Grong/tinyiothub" target="_blank" rel="noopener noreferrer" className="text-secondary hover:text-primary transition-colors homepage-nav-text">
                <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 24 24">
                  <path fillRule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" clipRule="evenodd" />
                </svg>
              </a>
              <a href="/signin" className="text-sm font-medium text-secondary hover:text-primary transition-colors homepage-nav-text">登录</a>
              <a href="/signin" className="rounded-lg bg-components-button-primary-bg text-components-button-primary-text px-5 py-2.5 text-sm font-semibold hover:bg-components-button-primary-bg-hover transition-all dark:opacity-90">免费试用</a>
            </div>
          </div>
        </div>
      </nav>

      {/* Hero Section */}
      <section className="relative pt-28 pb-20">
        <div className="absolute inset-0 -z-10 overflow-hidden">
          <div className="homepage-hero-glow-1 absolute top-0 left-1/2 -translate-x-1/2 w-[800px] h-[500px] bg-gradient-to-b from-blue-200/40 via-blue-100/20 to-transparent rounded-[100%] blur-3xl" />
          <div className="homepage-hero-glow-2 absolute top-10 left-20 w-64 h-64 bg-white/40 rounded-full blur-3xl backdrop-blur-xl border border-white/30" />
          <div className="homepage-hero-glow-3 absolute top-32 right-20 w-80 h-80 bg-indigo-200/30 rounded-full blur-3xl backdrop-blur-xl border border-white/30" />
          <div className="homepage-hero-glow-4 absolute bottom-20 left-1/3 w-72 h-72 bg-purple-200/20 rounded-full blur-3xl backdrop-blur-xl border border-white/30" />
        </div>

        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex justify-center mb-6">
            <div className="homepage-badge rounded-full border border-purple-200/50 bg-gradient-to-r from-purple-50/80 to-blue-50/80 backdrop-blur-xl px-5 py-2 text-sm text-purple-700 shadow-lg">
              <span className="relative inline-flex h-2 w-2 mr-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-purple-400 opacity-75" />
                <span className="relative inline-flex rounded-full h-2 w-2 bg-purple-500" />
              </span>
              内置人工智能 · 物联行业的 OpenAI
            </div>
          </div>

          <div className="text-center max-w-4xl mx-auto">
            <h1 className="text-5xl sm:text-6xl lg:text-7xl font-bold tracking-tight">
              <span className="text-gray-900 dark:text-white">构建下一代 </span>
              <span className="homepage-hero-gradient-text bg-gradient-to-r from-blue-600 to-indigo-600 bg-clip-text text-transparent">IoT 平台</span>
            </h1>
            <p className="homepage-hero-description mt-8 text-xl leading-8 text-gray-600 max-w-2xl mx-auto">
              轻量级、高性能、企业级的物联网边缘网关系统。基于 Rust + AI 构建，为工业物联网场景提供可靠的设备接入、数据采集和边缘计算能力。
            </p>

            <div className="mt-12 flex flex-col sm:flex-row items-center justify-center gap-4">
              <a href="/signin" className="w-full sm:w-auto rounded-xl bg-gradient-to-r from-blue-600 to-blue-700 px-8 py-4 text-base font-semibold text-white shadow-xl shadow-blue-600/40 hover:shadow-2xl transition-all flex items-center justify-center gap-2 dark:opacity-90">
                开始免费试用
                <ArrowRightIcon className="h-5 w-5" />
              </a>
              <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" className="w-full sm:w-auto rounded-xl border border-white/50 bg-white/60 backdrop-blur-xl px-8 py-4 text-base font-semibold text-gray-700 shadow-lg hover:bg-white/80 transition-all flex items-center justify-center gap-2 dark:border-white/20 dark:bg-slate-800/60 dark:text-gray-200 dark:hover:bg-slate-700/60">
                查看文档
              </a>
            </div>

            <div className="mt-16">
              <p className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-4">支持的协议</p>
              <div className="flex flex-wrap justify-center gap-3">
                {protocols.map((p) => (
                  <div key={p.name} className="homepage-protocol-badge rounded-xl bg-white/60 backdrop-blur-xl border border-white/50 px-5 py-2.5 shadow-lg">
                    <span className="text-sm font-medium text-gray-700">{p.name}</span>
                    <span className="text-sm text-gray-400 ml-1">{p.desc}</span>
                  </div>
                ))}
                <div className="homepage-protocol-9999 rounded-xl bg-gradient-to-r from-blue-50 to-indigo-50 backdrop-blur-xl border border-blue-200/50 px-5 py-2.5 shadow-lg">
                  <span className="text-sm font-medium bg-gradient-to-r from-blue-600 to-indigo-600 bg-clip-text text-transparent">9999+ 协议支持</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Stats */}
      <section className="py-16">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="rounded-3xl bg-white/50 backdrop-blur-2xl border border-white/60 shadow-xl p-8 lg:p-12 dark:bg-slate-900/60 dark:border-white/20 dark:shadow-none">
            <div className="grid grid-cols-2 lg:grid-cols-4 gap-8 lg:gap-16">
              {stats.map((stat) => (
                <div key={stat.label} className="text-center">
                  <div className="homepage-stats-value text-4xl lg:text-5xl font-bold bg-gradient-to-r from-gray-900 to-gray-700 bg-clip-text text-transparent">{stat.value}</div>
                  <div className="homepage-stats-label mt-2 text-sm text-gray-600">{stat.label}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* Edge Intelligence Agent Section */}
      <section className="py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="text-center max-w-3xl mx-auto mb-16">
            <div className="homepage-ai-badge inline-flex items-center gap-2 rounded-full border border-purple-200/50 bg-purple-50/60 backdrop-blur-xl px-4 py-1.5 text-sm text-purple-700 mb-6">
              <SparklesIcon className="h-4 w-4" />
              AI 驱动的新一代边缘计算
            </div>
            <h2 className="homepage-ai-heading text-3xl lg:text-5xl font-bold text-gray-900">
              边缘智能体
            </h2>
            <p className="homepage-ai-description mt-6 text-xl text-gray-600 leading-relaxed">
              <span className="homepage-ai-accent font-semibold text-purple-600">接入即自治，运行即自愈</span>
              <br />
              AI 原生自主型边缘计算平台，将大模型驱动的智能体嵌入边缘侧，从根本上重塑设备接入与运维流程
            </p>
          </div>

          {/* Core Value Cards */}
          <div className="grid md:grid-cols-2 lg:grid-cols-4 gap-6 mb-16">
            {coreFeatures.map((feature) => (
              <div key={feature.title} className="group rounded-2xl bg-white/60 backdrop-blur-xl border border-white/80 p-6 shadow-lg hover:shadow-2xl hover:bg-white/80 hover:border-purple-200/50 transition-all duration-300 dark:bg-slate-900/60 dark:border-white/20 dark:hover:bg-slate-800/60 dark:hover:border-white/10 dark:shadow-none">
                <div className={clsx(
                  "inline-flex rounded-xl p-3 mb-4",
                  feature.color === 'blue' && "bg-gradient-to-br from-blue-500 to-blue-600 text-white shadow-lg shadow-blue-500/30 dark:from-blue-600 dark:to-blue-700",
                  feature.color === 'green' && "bg-gradient-to-br from-green-500 to-green-600 text-white shadow-lg shadow-green-500/30 dark:from-green-600 dark:to-green-700",
                  feature.color === 'purple' && "bg-gradient-to-br from-purple-500 to-purple-600 text-white shadow-lg shadow-purple-500/30 dark:from-purple-600 dark:to-purple-700",
                  feature.color === 'orange' && "bg-gradient-to-br from-orange-500 to-orange-600 text-white shadow-lg shadow-orange-500/30 dark:from-orange-600 dark:to-orange-700",
                )}>
                  <feature.icon className="h-6 w-6" />
                </div>
                <h3 className="text-lg font-bold text-gray-900 dark:text-white">{feature.title}</h3>
                <p className="mt-2 text-sm text-gray-600 dark:text-gray-400">{feature.desc}</p>
              </div>
            ))}
          </div>

          {/* Feature Cards */}
          <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6">
            {agentFeatures.map((feature) => (
              <div key={feature.title} className="group rounded-2xl bg-white/60 backdrop-blur-xl border border-white/80 p-8 shadow-lg hover:shadow-2xl hover:bg-white/80 hover:border-purple-200/50 hover:-translate-y-1 transition-all duration-300 dark:bg-slate-900/60 dark:border-white/20 dark:hover:bg-slate-800/60 dark:hover:border-white/10 dark:shadow-none">
                <div className={clsx(
                  "inline-flex rounded-xl p-3 transition-transform duration-300 group-hover:scale-110",
                  feature.color === 'blue' && "bg-gradient-to-br from-blue-500 to-blue-600 text-white shadow-lg shadow-blue-500/30 dark:from-blue-600 dark:to-blue-700",
                  feature.color === 'green' && "bg-gradient-to-br from-green-500 to-green-600 text-white shadow-lg shadow-green-500/30 dark:from-green-600 dark:to-green-700",
                  feature.color === 'purple' && "bg-gradient-to-br from-purple-500 to-purple-600 text-white shadow-lg shadow-purple-500/30 dark:from-purple-600 dark:to-purple-700",
                  feature.color === 'orange' && "bg-gradient-to-br from-orange-500 to-orange-600 text-white shadow-lg shadow-orange-500/30 dark:from-orange-600 dark:to-orange-700",
                  feature.color === 'red' && "bg-gradient-to-br from-red-500 to-red-600 text-white shadow-lg shadow-red-500/30 dark:from-red-600 dark:to-red-700",
                  feature.color === 'indigo' && "bg-gradient-to-br from-indigo-500 to-indigo-600 text-white shadow-lg shadow-indigo-500/30 dark:from-indigo-600 dark:to-indigo-700",
                )}>
                  <feature.icon className="h-6 w-6" />
                </div>
                <h3 className="mt-6 text-xl font-semibold text-gray-900 dark:text-white">{feature.title}</h3>
                <p className="mt-3 text-gray-600 dark:text-gray-400 leading-relaxed">{feature.description}</p>
              </div>
            ))}
          </div>

          {/* Scenarios */}
          <div className="mt-16">
            <h3 className="text-xl font-bold text-gray-900 dark:text-white text-center mb-8">典型应用场景</h3>
            <div className="grid md:grid-cols-3 gap-6">
              {scenarios.map((scenario) => (
                <div key={scenario.title} className="homepage-scenario-card rounded-2xl bg-gradient-to-br from-purple-50/80 to-indigo-50/80 backdrop-blur-xl border border-purple-100/50 p-6 text-center hover:shadow-lg transition-all">
                  <h4 className="font-semibold text-gray-900">{scenario.title}</h4>
                  <p className="mt-2 text-sm text-gray-600">{scenario.desc}</p>
                </div>
              ))}
            </div>
          </div>

          {/* CTA */}
          <div className="mt-16 text-center">
            <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-r from-purple-600 to-indigo-600 px-8 py-4 text-base font-semibold text-white shadow-xl shadow-purple-600/40 hover:shadow-2xl transition-all dark:opacity-90">
              了解更多
              <ArrowRightIcon className="h-5 w-5" />
            </a>
          </div>
        </div>
      </section>

      {/* CTA Section */}
      <section className="py-24 relative">
        <div className="absolute inset-0 -z-10 overflow-hidden">
          <div className="absolute top-0 left-1/4 w-[500px] h-[500px] bg-gradient-to-r from-blue-500/20 to-indigo-500/20 rounded-full blur-3xl dark:from-blue-600/10 dark:to-indigo-600/10" />
          <div className="absolute bottom-0 right-1/4 w-[600px] h-[600px] bg-gradient-to-r from-indigo-500/20 to-purple-500/20 rounded-full blur-3xl dark:from-indigo-600/10 dark:to-purple-600/10" />
        </div>

        <div className="mx-auto max-w-4xl px-6 lg:px-8">
          <div className="rounded-3xl bg-white/60 backdrop-blur-2xl border border-white/50 shadow-2xl p-12 lg:p-16 text-center dark:bg-slate-900/60 dark:border-white/20 dark:shadow-none">
            <h2 className="text-3xl lg:text-5xl font-bold text-gray-900 dark:text-white">准备好开始了吗？</h2>
            <p className="mt-6 text-xl text-gray-600 dark:text-gray-400 max-w-2xl mx-auto">
              立即部署 TinyIoTHub，开启您的物联网之旅。开源免费，支持私有化部署。
            </p>
            <div className="mt-12 flex flex-col sm:flex-row items-center justify-center gap-4">
              <a href="/signin" className="w-full sm:w-auto rounded-xl bg-gradient-to-r from-blue-600 to-blue-700 px-8 py-4 text-base font-semibold text-white shadow-xl shadow-blue-600/40 hover:shadow-2xl transition-all flex items-center justify-center gap-2 dark:opacity-90">
                免费开始使用
                <ArrowRightIcon className="h-5 w-5" />
              </a>
              <a href="https://github.com/Grong/tinyiothub" target="_blank" rel="noopener noreferrer" className="w-full sm:w-auto rounded-xl border border-gray-200 bg-white/80 backdrop-blur-xl px-8 py-4 text-base font-semibold text-gray-700 shadow-lg hover:bg-white hover:shadow-xl transition-all flex items-center justify-center gap-2 dark:border-white/20 dark:bg-slate-800/60 dark:text-gray-200 dark:hover:bg-slate-700/60">
                <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 24 24">
                  <path fillRule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" clipRule="evenodd" />
                </svg>
                查看 GitHub
              </a>
            </div>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t border-gray-200/50 bg-white/50 backdrop-blur-xl py-12 dark:border-white/10 dark:bg-slate-900/60">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex flex-col lg:flex-row lg:items-center lg:justify-between gap-8">
            <div className="flex items-center gap-3">
              <img src={`${basePath}/logo.svg`} alt="logo" className="h-10 w-10 object-contain homepage-logo" />
              <div>
                <span className="text-lg font-bold text-gray-900 dark:text-white">TinyIoTHub</span>
                <p className="text-sm text-gray-500 dark:text-gray-400">开源物联网平台</p>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-6">
              <a href="https://github.com/Grong/tinyiothub" target="_blank" rel="noopener noreferrer" className="text-sm text-gray-600 hover:text-gray-900 transition-colors dark:text-gray-400 dark:hover:text-white">GitHub</a>
              <a href="/marketplace" className="text-sm text-gray-600 hover:text-gray-900 transition-colors dark:text-gray-400 dark:hover:text-white">市场</a>
              <a href="https://docs.tinyiothub.com" target="_blank" rel="noopener noreferrer" className="text-sm text-gray-600 hover:text-gray-900 transition-colors dark:text-gray-400 dark:hover:text-white">文档</a>
              <a href="/signin" className="text-sm text-gray-600 hover:text-gray-900 transition-colors dark:text-gray-400 dark:hover:text-white">登录</a>
            </div>
            <p className="text-sm text-gray-500 dark:text-gray-500">&copy; 2026 TinyIoTHub. All rights reserved. | <a href="https://beian.miit.gov.cn/" target="_blank" rel="noopener noreferrer" className="hover:text-gray-700 dark:hover:text-gray-300 transition-colors">粤ICP备2026029601号-2</a></p>
          </div>
        </div>
      </footer>
    </div>
  )
}
