---
name: TinyIoTHub
description: AI 时代的万物互联底座 — 自治机舱风格的深空 IoT 操作界面
colors:
  signal-blue: "#0098FF"
  signal-blue-hover: "#33B0FF"
  aurora-cyan: "#00d4ff"
  nebula-violet: "#7b61ff"
  teal-secondary: "#14b8a6"
  deep-space: "#12141a"
  surface-card: "#181b22"
  surface-elevated: "#1a1d25"
  surface-hover: "#262a35"
  glass-surface: "rgba(26, 29, 37, 0.65)"
  text-primary: "#e4e4e7"
  text-strong: "#fafafa"
  text-muted: "#71717a"
  border-subtle: "#27272a"
  ok-green: "#22c55e"
  warn-amber: "#f59e0b"
  danger-red: "#ef4444"
  info-blue: "#3b82f6"
typography:
  display:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', Roboto, sans-serif"
    fontWeight: 700
    letterSpacing: "-0.03em"
  title:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', Roboto, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    letterSpacing: "-0.02em"
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', Roboto, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "-0.02em"
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', Roboto, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    letterSpacing: "0.04em"
  mono:
    fontFamily: "'JetBrains Mono', ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace"
    fontSize: "13px"
    fontWeight: 500
rounded:
  sm: "6px"
  md: "10px"
  lg: "14px"
  xl: "20px"
  full: "9999px"
spacing:
  "1": "4px"
  "2": "8px"
  "3": "12px"
  "4": "16px"
  "5": "20px"
  "6": "24px"
  "7": "32px"
  "8": "40px"
  "9": "60px"
components:
  button-default:
    backgroundColor: "{colors.surface-hover}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "9px 16px"
  button-danger:
    backgroundColor: "rgba(239, 68, 68, 0.12)"
    textColor: "{colors.danger-red}"
    rounded: "{rounded.md}"
    padding: "9px 16px"
  card:
    backgroundColor: "{colors.surface-card}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.lg}"
    padding: "20px"
  input-field:
    backgroundColor: "{colors.surface-card}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.md}"
    padding: "11px 12px"
---

# Design System: TinyIoTHub

## Overview

**Creative North Star: "自治机舱 (The Autonomous Cockpit)"**

运维者坐在深空中的机舱控制台前：舱外是设备星云（深空蓝黑画布上的极光青与星云紫微光），舱内每个「物」都是可操控的终端。AI 是舱内的自动驾驶仪——它巡检、决策、执行，仪表盘的光晕与脉冲是它的呼吸；人保留接管权，审批与确认是仅剩的手动操纵杆。

氛围基调是**全面沉浸**：深空玻璃拟态与发光语言不分页面类型，工具页（表格/表单/设置）与沉浸页（首页/3D 孪生/聊天）共享同一套深空画布、玻璃表面与信号蓝渐变。沉浸不等于喧闹——动效有节制，持续动画只属于状态指示与沉浸场景，且全部服从 `prefers-reduced-motion`。

交互性格是**精致且克制**：表面静止，反馈通过颜色与光晕变化传达，不做位移动画。

**Key Characteristics:**
- 深空蓝黑画布 + 双层径向微光（青在左上、紫在右下，透明度 4–6%，数据永远是主角）
- 信号蓝为主角动作色，极光青→信号蓝→星云紫三段品牌渐变为高级表面的签名
- 无边框悬浮：表面无可见 border，阴影堆叠 + 内描边高光定义层级
- 玻璃拟态浮动层：65–72% 透明度 + 20px 模糊 + 180% 饱和
- 发光状态语言：状态点、开关、度量条用渐变 + 光晕 + 脉冲，禁用平涂
- 中英双语对等：字体栈内置 PingFang SC / Microsoft YaHei，数字与代码用 JetBrains Mono

## Colors

调色板的性格：深空为底，一束信号蓝划破黑暗，青紫渐变是机舱的舷窗光。以下值为暗色主题（默认 `:root`）；亮色主题（`:root[data-theme="light"]`）完整存在于 `base.css`，品牌渐变与信号蓝在两主题间保持一致。

### Primary
- **信号蓝 Signal Blue** (#0098FF): 唯一主角动作色——主要按钮（渐变中站）、链接、焦点环、选中态、告警外的所有"可交互"语义。hover 变体 (#33B0FF)。
- **极光青 Aurora Cyan** (#00d4ff): 品牌渐变起点、高强度光晕色、状态点渐变成员。不单独作为文本色。
- **星云紫 Nebula Violet** (#7b61ff): 品牌渐变终点、深空画布右下微光。亮色主题不得丢弃此端点。

### Secondary
- **深潭青 Teal** (#14b8a6): 辅助强调色，用于需要与信号蓝区分的第一二数据系列/分类标签， subdued 变体 (70%/15% 透明度） 作填充。

### Neutral
- **深空蓝黑 Deep Space** (#12141a): 页面基底，叠加青/紫径向微光构成 `--bg-deep-space`。
- **舱面卡 Surface Card** (#181b22): 卡片/输入/弹层默认表面。
- **高架面 Elevated** (#1a1d25): 浮起一级的面板与玻璃底色来源。
- **悬停面 Hover** (#262a35): 默认按钮底、hover 态表面。
- **玻璃面 Glass** (rgba(26,29,37,0.65)): 顶栏、模态、下拉等浮动层的半透明底。
- **正文 Text Primary** (#e4e4e7)、**强文本 Text Strong** (#fafafa)、**弱化文本 Muted** (#71717a)、**细分隔 Border Subtle** (#27272a)。

### Semantic
- **OK 绿** (#22c55e)、**警戒琥珀** (#f59e0b)、**危险红** (#ef4444)、**信息蓝** (#3b82f6)：各带 12% 填充 (subtle)、75% 弱化 (muted)、8px 光晕 (glow-sm) 三件套。

### Named Rules
**The One Gradient Rule.** 品牌渐变只有一条：极光青 → 信号蓝 → 星云紫（135deg）。主要按钮、度量条、统计数字、状态点共用它；不得发明第二条品牌渐变，亮色主题不得砍掉紫端点。

**The Signal Conservation Rule.** 信号蓝只标记"可交互/可聚焦"——按钮、链接、选中、焦点环。装饰性着色用弱化语义色，不让信号蓝贬值。

## Typography

**Display/Body Font:** 系统栈 (-apple-system, BlinkMacSystemFont, "PingFang SC", "Microsoft YaHei", "Segoe UI", Roboto)
**Label/Mono Font:** JetBrains Mono (ui-monospace 回退）

**Character:** 一套双语系统栈走天下——中文 PingFang/雅黑与西文系统字体无缝混排，标题用 700 字重 + 负字距制造仪器般的紧凑感；JetBrains Mono 是数据的母语：设备 ID、命令、指标值、快捷键徽章一律等宽。

### Hierarchy
- **Display** (700, letter-spacing -0.03em): h1/h2/h3 与品牌标题。尺寸由视图自定，字距不变量。
- **Title** (600, 15px, -0.02em): 卡片标题、面板标题。
- **Body** (400, 14px, line-height 1.55, -0.02em): 全局正文基准，紧凑运维密度。
- **Label** (500, 11px, uppercase, +0.04em): 统计标签、表头、分组小标题。
- **Mono** (500, 13px): 命令文本、ID、键帽徽章（`.btn-kbd`）、审批模态的元数据值。
- **Stat Value** (700, 24px, -0.03em, line-height 1.1): 大数字，可用渐变文字处理。

### Named Rules
**The Mono-for-Machine Rule.** 凡机器产出或机器消费的字符串（命令、ID、topic、时间戳、快捷键）用 JetBrains Mono；凡人话用系统栈。两种字体不混排在同一句子里。

## Layout

驾驶舱布局：左侧固定导航 + 顶部玻璃状态栏 + 内容区，全部按 Workspace 隔离的数据密度设计。

- **Shell 网格**: 导航列 (220px) + 内容列，顶栏行 (56px)。导航可折叠至 0（`--nav-collapsed` / chat-focus 模式），折叠过渡 200ms ease-out。
- **内容密度**: 紧凑运维密度——正文 14px、按钮 13px、卡片内边距 20px (--space-5)。不追求宽松呼吸感，追求单屏信息量。
- **间距节奏**: 4px 基数的九级阶梯 (4/8/12/16/20/24/32/40/60)，视图内禁止硬编码像素（1px 边框、媒体查询断点、画布尺寸除外）。
- **响应式**: 主断点 1100px（布局收敛），辅助 600px / 400px（移动覆写在 `layout.mobile.css`）。
- **层级**: 九级 z-index 阶梯（base 0 → dropdown 100 → sticky 200 → modal-backdrop 300 → modal 400 → popover 500 → toast 600 → tooltip 700 → loading 800）。

## Elevation & Depth

**本系统不使用可见边框表达层级（已确认为不变量）。** 层级由三件事表达：阴影堆叠的深度、内描边高光（inset 0 0 0 1px 白 4-6%）、玻璃模糊。光晕（glow）不是装饰，是状态语言——hover、focus、在线、告警时才出现。

### Shadow Vocabulary
- **静止卡** (`box-shadow: 0 1px 4px rgba(0,0,0,0.12)`): 卡片/输入默认，几乎不可察觉。
- **悬停卡** (`0 4px 24px rgba(0,0,0,0.24)`): hover 时深度加深一档。
- **高架面** (`0 12px 28px rgba(0,0,0,0.35) + 内高光`): 模态、下拉。
- **最高层** (`0 24px 48px rgba(0,0,0,0.4) + 内高光`): 全屏对话、关键弹层。
- **悬浮卡（落地页）**: 四层堆叠 (4px/16px/40px 三层投影 + inset 高光），hover 整体加深 + 表面上移一阶亮度。
- **焦点环** (`0 0 0 2px bg, 0 0 0 4px #0098FF`): 双层描边式焦点；加强版追加 20px 信号蓝光晕。
- **语义光晕** (`0 0 8px <语义色 50%>`): 状态点、开关、在线指示专用。

### Named Rules
**The Borderless Levitation Rule.** 卡片、面板、按钮永不画可见 `border`；边缘由 inset 高光 + 阴影堆叠塑造。输入框是唯一例外（保留 1px 细分隔线，focus 时变为信号蓝 + 3px 光晕环）。

**The Glow-Means-State Rule.** 光晕只在表达状态时出现：hover 加深、focus 环绕、状态点脉冲、开关导通。静态装饰元素不发光。

## Shapes

形式语言：友好的大圆角 + 全圆角药丸。半径阶梯 6/10/14/20/全圆——小控件 (6px)、默认卡片与输入 (10px)、模态与面板 (14px)、大卡与对话 (20px)、药丸/头像/开关/状态点 (9999px)。无尖角语言，无斜切，无异形裁剪；主题切换是唯一的圆形 clip-path 动画（从点击位置扩散的圆）。

## Components

### Buttons
无边框、精致克制：仅颜色与光晕反馈，无位移。
- **Shape:** 友好圆角 (10px)，小号 (sm) 与图标按钮同半径
- **Primary:** 品牌渐变底 (135deg 青→蓝→紫） + 白字 + 2px/10px 信号蓝光晕 (padding 9px 16px, 字号 13px/500)
- **Default:** 悬停面灰底 (#262a35) + 正文色字
- **Danger / Success / Warning:** 12% 语义色填充 + 语义色文字，hover 填充加深至 15%
- **Icon:** 36×36 方形，16px 线性图标 (stroke 1.5px, round caps)
- **Disabled:** 50% 透明度，不另配色

### Cards / Containers
- **Corner Style:** 大圆角 (14px)
- **Background:** 舱面卡色 (#181b22) + 4px 玻璃模糊
- **Border:** 无（内高光替代）；高级卡可用渐变描边变体（`::before` + mask-composite 的 1px 白 12%→3% 渐变边）
- **Shadow Strategy:** 静止卡 → 悬停卡（见 Elevation）
- **Internal Padding:** 20px (--space-5)
- **Stat 变体:** 紧凑内边距 (12px 16px)，标签 11px 大写，数值 24px/700，数值可渐变文字

### Inputs / Fields
- **Style:** 舱面卡底 + 1px 细分隔线 (#27272a) + 默认圆角 (10px) + padding (11px 12px)
- **Focus:** 边框变信号蓝 + 3px 15% 信号蓝光晕环
- **Hover（未聚焦）:** 边框升至 #52525b
- **Disabled:** 50% 透明度；placeholder 用弱化文本色

### Toggle Switch
药丸轨道 (48×24，大号 56×28)，关态弱化灰，开态 OK 绿 + 8px 绿光晕，白色圆形滑块平移，过渡 350ms。

### Status Dots
小圆点 = 渐变填充 + 8px 语义光晕 + 2s 脉冲呼吸。变体：success（青→绿）、warning（琥珀→红）、danger（红）、glow（信号蓝）。**禁用平涂色。**

### Metric Bars
品牌渐变填充 + 10px 光晕 + 3s 扫光（白色 25% 高光横向往复）。持续动画，reduced-motion 时关闭。

### Modals / Dialogs
深色遮罩 (rgba(0,0,0,0.5~0.8) + 4px 模糊） 上的高架卡 (14px 圆角、20px 内边距、scale-in 200ms)。命令/元数据值用 Mono。必须满足可访问性四件套：role="dialog"、aria-label、Esc 关闭、焦点圈禁与归还。

### Navigation
- **Shell:** 220px 左侧导航（可折叠至 0）+ 56px 玻璃顶栏（65% 深底 + 12px 模糊 + 160% 饱和）
- **Brand:** 28px logo + 16px/700 标题 + 10px 大写副标
- **Theme Toggle:** 三格药丸轨道（28px 格），信号蓝圆形指示器滑动切换亮/暗/跟随系统

### Signature: Deep-Space Canvas
页面基底不是纯色：`--bg-deep-space` = 左上极光青 6% 径向光斑 + 右下星云紫 4% 径向光斑 + 深空蓝黑。亮色主题同构降为 4%/3%。数据与卡片浮在微光之上，微光永不抢读。

## Do's and Don'ts

### Do:
- **Do** 所有新表面走无边框悬浮：阴影堆叠 + inset 高光，不画 `border`（输入框除外）。
- **Do** 状态指示用渐变 + 光晕 + 脉冲的状态点，语义色三件套（fill/muted/glow）配套使用。
- **Do** 主要动作、统计大数字、度量条复用唯一品牌渐变 (135deg, #00d4ff → #0098FF → #7b61ff)。
- **Do** 持续动画（脉冲/扫光/粒子）包进 `@media (prefers-reduced-motion: reduce)` 禁用块。
- **Do** 机器字符串（命令、ID、topic）用 JetBrains Mono，键帽徽章用 `.btn-kbd` 样式。
- **Do** 新视图把重复 5 次以上的颜色/阴影抽成视图作用域 token（`view-name --name-*`），全局 token 优先复用 `--space-*` / `--radius-*` / `--duration-*`。

### Don't:
- **Don't** 给卡片/按钮/面板加可见边框或平涂灰边——层级靠阴影与内高光。
- **Don't** 发明第二条品牌渐变，或在亮色主题丢弃星云紫端点。
- **Don't** 用平涂色做状态点、开关、度量条——渐变 + 光晕是状态语言。
- **Don't** 在静态装饰上使用光晕或持续动画；动效有节制，工具页仅限状态过渡。
- **Don't** 使用 hover 位移（translateY）动画——反馈仅通过颜色与光晕变化。
- **Don't** 在视图 CSS 中硬编码间距/圆角/时长像素值（1px 边框、断点、画布尺寸除外）。
