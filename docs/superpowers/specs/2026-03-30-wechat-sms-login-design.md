# 微信登录与手机验证码登录设计方案

**日期**: 2026-03-30
**状态**: 已批准
**版本**: v1.0

---

## 一、需求概述

新增两种登录方式：
1. **微信登录**：Web 前端扫码 OAuth 2.0 登录
2. **手机验证码登录**：通过阿里云 SMS 发送验证码登录

两种方式均支持新用户直接注册（混合模式），但手机号必须唯一绑定一个账户。

---

## 二、数据模型

### 2.1 users 表变更

```sql
ALTER TABLE users ADD COLUMN phone_number VARCHAR(20) UNIQUE;
```

### 2.2 user_auth_links 表

```sql
CREATE TABLE user_auth_links (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    provider VARCHAR(20) NOT NULL,  -- 'wechat', 'sms'
    provider_user_id VARCHAR(128) NOT NULL,  -- openid / phone_number
    created_at TIMESTAMP DEFAULT NOW(),
    UNIQUE(provider, provider_user_id)
);
```

---

## 三、API 端点

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/auth/wechat/qrcode` | GET | 获取微信登录二维码 URL |
| `/api/v1/auth/wechat/callback` | GET | 微信 OAuth 回调（扫码后跳转）|
| `/api/v1/auth/sms/send` | POST | 发送短信验证码 |
| `/api/v1/auth/sms/login` | POST | 手机号 + 验证码登录 |
| `/api/v1/auth/bind/phone` | POST | 绑定手机号（需已登录） |
| `/api/v1/auth/bind/wechat` | POST | 绑定微信（需已登录，扫码授权）|

---

## 四、微信 OAuth 流程

```
用户点击微信登录
       ↓
前端调用 /api/v1/auth/wechat/qrcode 获取二维码 URL
       ↓
前端渲染二维码（调用微信 H5 SDK 或自行构造 URL）
       ↓
用户扫码并确认授权
       ↓
微信回调到 /api/v1/auth/wechat/callback?code=xxx&state=xxx
       ↓
后端用 code 换取 openid
       ↓
查询 user_auth_links，匹配 openid：
    - 已存在 → 生成 JWT，返回登录成功
    - 不存在 → 创建新用户 + auth_link，返回 JWT（首次登录）
       ↓
前端存储 JWT，完成登录
```

---

## 五、手机验证码登录流程

```
用户输入手机号，点击获取验证码
       ↓
前端调用 /api/v1/auth/sms/send { phone: "138xxxx" }
       ↓
后端校验频率限制（每天 5 条，间隔 90 秒）
       ↓
通过 → 生成 6 位验证码，存入 Redis（有效期 5 分钟）
       ↓
调用阿里云 SMS API 发送短信
       ↓
用户输入验证码，点击登录
       ↓
前端调用 /api/v1/auth/sms/login { phone, code }
       ↓
后端校验 Redis 中的验证码
       ↓
匹配成功，查询/创建用户和 auth_link，生成 JWT
```

---

## 六、防机器人机制

### 6.1 分级触发

**第一级：正常用户（无感知）**
- 首次请求直接发送，无需额外操作
- 发送间隔 90 秒（前端的 UX 限制，后端也校验）

**第二级：疑似异常（触发图形验证码）**
- 同一手机号 5 分钟内发了 2 次还没验证
- 同一 IP 5 分钟内发了 3 次

**第三级：高风险（直接拦截）**
- 同一 IP 当天发送超过 10 条
- IP 命中风险名单（IDC IP、代理 IP）

### 6.2 技术实现

- 集成腾讯防水墙（国内推荐）
- 备选：Google reCAPTCHA v2

### 6.3 频率限制参数

| 限制项 | 值 |
|--------|-----|
| 同手机号每天最多发送 | 5 条 |
| 发送间隔 | 90 秒 |
| 同 IP 5 分钟内最多发送 | 3 条 |
| 验证码有效期 | 5 分钟 |
| 验证码错误容忍次数 | 3 次 |

---

## 七、安全设计

| 机制 | 说明 |
|------|------|
| 微信 code 一次性 | 每个 code 只能使用一次，防止重放 |
| 短信验证码 Redis 存储 | 5 分钟过期，精确匹配后删除 |
| 频率限制 | Redis 记录发送次数，每日/间隔双重校验 |
| JWT Access Token | 2 小时过期 |
| Refresh Token | 30 天有效期，支持旋转 |

---

## 八、错误处理

| 场景 | 返回 | 说明 |
|------|------|------|
| 验证码已过期 | `{"code": -1, "msg": "验证码已过期，请重新获取"}` | Redis 中已删除 |
| 验证码错误 | `{"code": -1, "msg": "验证码错误"}` | 最多错 3 次，错满 3 次后删除验证码 |
| 发送太频繁 | `{"code": -1, "msg": "操作太频繁，请 90 秒后重试"}` | 间隔不足 90 秒 |
| 超过每日限额 | `{"code": -1, "msg": "今日发送次数已用完，请明天再试"}` | 同手机号每天最多 5 条 |
| 微信 code 已使用 | `{"code": -1, "msg": "授权已过期，请重新扫码"}` | code 只能使用一次 |
| 需要人机验证 | `{"code": 1001, "msg": "请先完成验证"}` | 返回特定 code，前端引导验证 |

---

## 九、Redis Key 设计

```
sms:code:{phone}           # 验证码，TTL 5 分钟
sms:count:daily:{phone}    # 手机号当天发送次数，TTL 到次日 0 点
sms:count:ip:{ip}          # IP 5 分钟内发送次数，TTL 5 分钟
sms:verify:fail:{phone}    # 验证码错误次数，TTL 5 分钟
wechat:state:{state}       # 微信 OAuth state 参数，TTL 5 分钟
```

---

## 十、阿里云 SMS 集成

- SDK：`dysmsapi20170525`（阿里云 OpenAPI SDK for Rust）
- 签名方式：RSA2（推荐）或 HMAC-SHA256
- 模板变量：`${code}` 格式

---

## 十一、前端集成点

```
web/
├── service/auth.ts          # 新增：微信登录、短信登录 API 封装
├── hooks/
│   ├── use-wechat-login.ts  # 新增：微信扫码登录 hook
│   └── use-sms-login.ts     # 新增：短信验证码登录 hook
└── app/(auth)/
    ├── login/page.tsx       # 改造：增加微信/短信登录入口
    └── bind/page.tsx        # 新增：绑定手机/微信页面
```

---

## 十二、测试要点

| 测试场景 | 预期 |
|----------|------|
| 正常发送验证码 | 短信收到 6 位数字验证码 |
| 90 秒内重复发送 | 返回"操作太频繁" |
| 输入错误验证码 | 返回"验证码错误"，错 3 次后验证码失效 |
| 验证码过期后输入 | 返回"验证码已过期" |
| 微信扫码授权成功 | 跳转页面 + JWT Token |
| 微信 code 重复使用 | 返回"授权已过期" |

---

## 十三、待完成事项

- [ ] 申请微信开放平台 AppID 和 AppSecret
- [ ] 申请阿里云 SMS 签名和模板
- [ ] 集成腾讯防水墙 AppID 和 AppSecret
