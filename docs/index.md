---
layout: home

hero:
  name: "TinyIoTHub"
  text: "AI 时代的万物互联底座"
  tagline: 以「物」为本体的 AIoT 平台 — 构建物、通过驱动更新实时数据，剩下的交给 AI
  actions:
    - theme: brand
      text: 快速开始
      link: /getting-started/
    - theme: alt
      text: API 参考
      link: /api/

features:
  - title: 以「物」为本体
    details: 设备、空间、产线统一建模为层级化「物」，属性/事件/操作 + 知识文档一体定义——实现万物智能只需三步：① 构建物 ② 驱动更新实时数据 ③ 剩下的交给 AI
  - title: 多协议设备接入
    details: 内置 Modbus RTU/TCP、ONVIF、SNMP、MQTT 驱动，AI 辅助匹配与生成驱动代码，JSON 模板一键创建设备
  - title: L0-L3 自愈引擎
    details: system/device/task 三级探针自动检测故障并恢复，心跳探针主动巡检，Cron 定时任务调度
  - title: 自治运维（Thing Agent Loop）
    details: AI 被设备事件、定时巡检或用户指令唤醒，基于物本体自主诊断并操作设备——三态策略门（off/diagnose/act）管控权限，行动后回读验证，全程审计可追溯
  - title: 自然语言运维
    details: 用日常语言配置设备、查询状态、排查故障。内嵌 MCP Server，支持 Claude Desktop、Cursor 直接连接
  - title: 规则引擎
    details: 阈值、范围、变化、持续时间、组合五种条件类型，灵活配置告警和自动化规则
  - title: 沉浸式工作空间
    details: 3D 数字孪生场景 + AI 数据洞察面板，自然语言驱动设备操作，实时查看 AI 执行过程
  - title: 轻量部署
    details: 单进程运行，~80MB 内存占用，SQLite 免外部依赖，开源 MIT 协议，支持私有化
---
