import { defineConfig } from 'vitepress'

export default defineConfig({
  title: "TinyIoTHub | 文档",
  description: "专业物联网边缘网关系统",
  lang: 'zh-CN',
  appearance: 'dark',

  ignoreDeadLinks: true,

  head: [
    ['link', { rel: 'icon', href: '/favicon.ico' }]
  ],

  themeConfig: {
    // Logo 链接到主站
    logoLink: 'https://tinyiothub.com',

    // 导航栏
    nav: [
      { text: '首页', link: '/' },
      { text: '快速开始', link: '/getting-started/' },
      { text: '用户指南', link: '/guide/' },
      { text: 'API 参考', link: '/api/' },
      { text: '驱动开发', link: '/drivers/' },
    ],

    // 侧边栏
    sidebar: {
      '/': [
        {
          text: '入门指南',
          items: [
            { text: '快速开始', link: '/getting-started/' },
            { text: '安装部署', link: '/getting-started/installation' },
            { text: '配置说明', link: '/getting-started/configuration' },
          ]
        },
        {
          text: '用户指南',
          items: [
            { text: '设备管理', link: '/guide/devices' },
            { text: '驱动管理', link: '/guide/drivers' },
            { text: '告警管理', link: '/guide/alarms' },
            { text: '用户管理', link: '/guide/users' },
          ]
        },
        {
          text: '开发指南',
          items: [
            { text: 'API 接口', link: '/api/' },
            { text: '驱动开发', link: '/drivers/' },
            { text: 'MQTT 协议', link: '/api/mqtt' },
          ]
        },
        {
          text: 'AI 集成',
          collapsed: true,
          items: [
            { text: 'MCP 设计', link: '/mcp-design' },
            { text: 'MCP 参数', link: '/mcp-parameters' },
            { text: 'MCP 需求', link: '/mcp-requirements' },
          ]
        },
        {
          text: '技术文档',
          collapsed: true,
          items: [
            { text: '事件架构', link: '/technical/event-handler-architecture' },
            { text: '告警概览', link: '/technical/alarm-module-overview' },
            { text: '告警进度', link: '/technical/alarm-implementation-progress' },
            { text: 'Docker 优化', link: '/technical/docker-build-optimization' },
          ]
        },
        {
          text: '部署指南',
          items: [
            { text: 'Docker 部署', link: '/deployment/docker' },
            { text: '单进程部署', link: '/deployment/single-process' },
            { text: '鸿蒙部署', link: '/deployment/harmonyos' },
          ]
        }
      ]
    },

    // 社交链接
    socialLinks: [
      { icon: 'github', link: 'https://github.com/Grong/tinyiothub' }
    ],

    // 页脚
    footer: {
      message: '基于 Rust 的高性能物联网边缘网关系统',
      copyright: 'MIT License © 2026 TinyIoTHub | <a href="https://beian.miit.gov.cn/" target="_blank" rel="noopener noreferrer" style="text-decoration: none;">粤ICP备2026029601号-2</a>'
    },

    // 搜索
    search: {
      provider: 'local'
    },

    // outline
    outline: 'deep'
  },

  vite: {
    resolve: {
      alias: {
        '@': '/src'
      }
    }
  }
})
