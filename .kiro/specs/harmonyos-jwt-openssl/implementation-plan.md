# HarmonyOS JWT/OpenSSL 实施计划

## 概述

本文档提供了实施 HarmonyOS JWT/OpenSSL 解决方案的详细步骤和时间表。

## 阶段划分

### 阶段 1: 环境准备和 ohos-openssl 集成 (1-2 天)

#### 任务 1.1: 克隆和配置 ohos-openssl
- [ ] 克隆 ohos-openssl 仓库到 `vendor/ohos-openssl`
- [ ] 验证预编译库文件存在（arm64-v8a）
- [ ] 更新 .gitignore 排除 vendor 目录（可选）
- [ ] 创建 vendor/README.md 说明第三方库

**命令:**
```powershell
git clone https://github.com/ohos-rs/ohos-openssl vendor/ohos-openssl
```

#### 任务 1.2: 更新 Cargo.toml
- [ ] 添加 openssl 依赖
- [ ] 配置 target-specific 依赖
- [ ] 测试依赖解析

**修改:**
```toml
[dependencies]
openssl = { version = "0.10", optional = true }

[target.'cfg(target_env = "ohos")'.dependencies]
openssl = "0.10"

[features]
harmonyos = ["openssl"]
```

#### 任务 1.3: 更新构建脚本
- [ ] 在 build-backend.ps1 中添加 ohos-openssl 检查
- [ ] 设置 AARCH64_UNKNOWN_LINUX_OHOS_OPENSSL_DIR 环境变量
- [ ] 添加错误处理和友好提示
- [ ] 测试构建脚本

**验收:** 构建脚本能够检测 ohos-openssl 并设置正确的环境变量

---

### 阶段 2: OpenSSL JWT 实现 (2-3 天)

#### 任务 2.1: 创建 jwt_openssl 模块
- [ ] 创建 `src/shared/security/jwt_openssl.rs`
- [ ] 实现 Base64URL 编码/解码函数
- [ ] 实现 HMAC-SHA256 签名函数
- [ ] 实现 JWT header 和 payload 构建

**文件结构:**
```rust
// src/shared/security/jwt_openssl.rs
pub fn create_jwt_with_openssl(payload: AuthPayload) -> Result<AuthBody, String>;
pub fn verify_jwt_with_openssl(token: &str) -> Result<Claims, String>;
fn sign_with_hmac_sha256(data: &str, secret: &str) -> Result<String, String>;
fn base64url_encode(data: &[u8]) -> String;
fn base64url_decode(data: &str) -> Result<Vec<u8>, String>;
```

#### 任务 2.2: 实现 JWT 创建功能
- [ ] 实现 create_jwt_with_openssl 函数
- [ ] 构建标准 JWT header (alg: HS256, typ: JWT)
- [ ] 构建 claims payload
- [ ] 使用 OpenSSL HMAC-SHA256 签名
- [ ] 组合 header.payload.signature
- [ ] 添加错误处理

**测试点:**
- 生成的 token 格式正确（三部分，用 . 分隔）
- header 和 payload 可以正确解码
- 签名长度正确

#### 任务 2.3: 实现 JWT 验证功能
- [ ] 实现 verify_jwt_with_openssl 函数
- [ ] 分割 token 为三部分
- [ ] 验证格式正确性
- [ ] 重新计算签名并比较（常量时间）
- [ ] 解析 payload 为 Claims
- [ ] 检查过期时间
- [ ] 添加详细错误信息

**测试点:**
- 正确的 token 能够验证通过
- 错误的签名被拒绝
- 过期的 token 被拒绝
- 格式错误的 token 被拒绝

#### 任务 2.4: 集成到主 JWT 模块
- [ ] 修改 `src/shared/security/jwt.rs`
- [ ] 添加 jwt_openssl 模块引用
- [ ] 在 create_jwt 中添加 HarmonyOS 分支
- [ ] 在 validate_jwt 中添加 HarmonyOS 分支
- [ ] 保持现有 jwt-simple 实现不变
- [ ] 添加日志标识使用的实现

**代码结构:**
```rust
// src/shared/security/jwt.rs
#[cfg(feature = "harmonyos")]
mod jwt_openssl;

pub fn create_jwt(payload: AuthPayload) -> Result<AuthBody, String> {
    if is_harmonyos() {
        tracing::info!("🔐 Using OpenSSL-based JWT for HarmonyOS");
        jwt_openssl::create_jwt_with_openssl(payload)
    } else {
        tracing::debug!("🔐 Using jwt-simple for standard platforms");
        // 现有实现
    }
}
```

**验收:** 代码能够在 HarmonyOS 和非 HarmonyOS 环境下编译通过

---

### 阶段 3: 测试和验证 (2-3 天)

#### 任务 3.1: 单元测试
- [ ] 创建 `tests/jwt_openssl_test.rs`
- [ ] 测试 JWT 创建功能
- [ ] 测试 JWT 验证功能
- [ ] 测试过期场景
- [ ] 测试无效签名场景
- [ ] 测试格式错误场景
- [ ] 测试边界条件

**测试用例:**
```rust
#[test]
fn test_create_and_verify_jwt_openssl()
#[test]
fn test_expired_token_rejected()
#[test]
fn test_invalid_signature_rejected()
#[test]
fn test_malformed_token_rejected()
#[test]
fn test_jwt_compatibility_between_implementations()
```

#### 任务 3.2: 本地编译测试
- [ ] 在 Windows 上编译 HarmonyOS 目标
- [ ] 验证 OpenSSL 正确链接
- [ ] 检查二进制文件大小
- [ ] 验证没有编译警告

**命令:**
```powershell
.\scripts\build-backend.ps1
```

#### 任务 3.3: HarmonyOS 设备测试
- [ ] 部署到 HarmonyOS 设备
- [ ] 测试登录功能
- [ ] 验证 token 生成
- [ ] 验证 token 验证
- [ ] 测试 API 调用（带 token）
- [ ] 检查日志输出
- [ ] 验证没有 Signal 11 崩溃

**测试脚本:**
```powershell
.\scripts\deploy.ps1
.\scripts\check-status.ps1
```

#### 任务 3.4: 集成测试
- [ ] 测试完整登录流程
- [ ] 测试 token 刷新（如果实现）
- [ ] 测试登出功能
- [ ] 测试并发登录
- [ ] 测试长时间运行稳定性

**验收:** 所有测试通过，系统在 HarmonyOS 上稳定运行

---

### 阶段 4: 安全性增强 (1-2 天)

#### 任务 4.1: 密钥管理
- [ ] 实现从环境变量读取 JWT_SECRET
- [ ] 实现从配置文件读取密钥
- [ ] 添加密钥强度验证
- [ ] 在生产环境强制使用强密钥
- [ ] 添加密钥轮换支持（可选）

**配置:**
```toml
# app_settings.toml
[security]
jwt_secret = "${JWT_SECRET}"  # 从环境变量读取
jwt_expiration = 86400  # 24小时
```

#### 任务 4.2: 安全加固
- [ ] 实现常量时间签名比较（使用 `subtle::ConstantTimeEq`，禁止手动字节比较）
- [ ] 添加 token 黑名单支持（可选）
- [ ] 限制 token 有效期
- [ ] 添加 token 使用次数限制（可选）
- [ ] 防止日志泄露敏感信息

#### 任务 4.3: 安全审计
- [ ] 代码安全审查
- [ ] 依赖安全扫描
- [ ] 渗透测试（基础）
- [ ] 文档安全最佳实践

**验收:** 通过安全审查，没有明显的安全漏洞

---

### 阶段 5: 性能优化 (1-2 天)

#### 任务 5.1: 性能基准测试
- [ ] 创建性能测试脚本
- [ ] 测试 JWT 创建性能
- [ ] 测试 JWT 验证性能
- [ ] 对比 OpenSSL 和 jwt-simple 性能
- [ ] 测试高并发场景

**基准目标:**
- JWT 创建: < 10ms
- JWT 验证: < 5ms
- 并发 100 请求/秒: 稳定

#### 任务 5.2: 性能优化
- [ ] 优化 Base64 编码/解码
- [ ] 缓存 OpenSSL 上下文（如果可能）
- [ ] 减少内存分配
- [ ] 优化字符串操作
- [ ] 使用 lazy_static 缓存常量

#### 任务 5.3: 监控和指标
- [ ] 添加 JWT 操作计数器
- [ ] 添加 JWT 操作延迟指标
- [ ] 添加错误率统计
- [ ] 集成到系统监控

**验收:** 性能满足目标，没有明显的性能瓶颈

---

### 阶段 6: 文档和部署 (1-2 天)

#### 任务 6.1: 技术文档
- [ ] 创建 `docs/HARMONYOS_JWT_OPENSSL.md`
- [ ] 文档化 OpenSSL 集成过程
- [ ] 文档化 JWT 实现细节
- [ ] 添加架构图和流程图
- [ ] 添加代码示例

#### 任务 6.2: 部署文档
- [ ] 更新 `HARMONYOS_DEPLOYMENT.md`
- [ ] 添加 ohos-openssl 安装步骤
- [ ] 添加环境变量配置说明
- [ ] 添加故障排查指南
- [ ] 添加性能调优建议

#### 任务 6.3: 用户文档
- [ ] 更新 README.md
- [ ] 添加 HarmonyOS 部署章节
- [ ] 更新 CHANGELOG.md
- [ ] 创建发布说明

#### 任务 6.4: 部署验证
- [ ] 在测试环境部署
- [ ] 验证部署流程
- [ ] 验证文档准确性
- [ ] 收集反馈并改进

**验收:** 文档完整、准确，部署流程清晰

---

### 阶段 7: 数据库问题调查（可选，独立进行）

#### 任务 7.1: 问题复现和分析
- [ ] 创建最小化测试用例
- [ ] 测试不同的 SQLite 配置
- [ ] 测试不同的文件路径
- [ ] 分析崩溃日志和堆栈
- [ ] 研究 HarmonyOS 文件系统特性

#### 任务 7.2: 解决方案探索
- [ ] 测试 SQLite bundled vs system
- [ ] 测试 WAL 模式 vs DELETE 模式
- [ ] 测试内存数据库
- [ ] 研究 HarmonyOS 原生数据库 API
- [ ] 考虑外部数据库方案

#### 任务 7.3: 实施和验证
- [ ] 实施选定的解决方案
- [ ] 测试数据库写入功能
- [ ] 验证没有 Signal 11
- [ ] 性能测试
- [ ] 文档化解决方案

**注意:** 此阶段可以与其他阶段并行进行，或者推迟到后续版本

---

## 时间表

| 阶段 | 任务 | 预计时间 | 依赖 |
|------|------|----------|------|
| 1 | 环境准备和 ohos-openssl 集成 | 1-2 天 | - |
| 2 | OpenSSL JWT 实现 | 2-3 天 | 阶段 1 |
| 3 | 测试和验证 | 2-3 天 | 阶段 2 |
| 4 | 安全性增强 | 1-2 天 | 阶段 3 |
| 5 | 性能优化 | 1-2 天 | 阶段 3 |
| 6 | 文档和部署 | 1-2 天 | 阶段 4, 5 |
| 7 | 数据库问题调查（可选） | 2-3 天 | - |

**总计:** 8-14 天（不包括数据库问题调查）

## 里程碑

### 里程碑 1: OpenSSL 集成完成
- ohos-openssl 正确配置
- 项目能够编译通过
- OpenSSL 库正确链接

**日期:** 第 2 天

### 里程碑 2: JWT 功能实现
- JWT 创建和验证功能完成
- 单元测试通过
- 代码审查完成

**日期:** 第 5 天

### 里程碑 3: HarmonyOS 设备验证
- 在 HarmonyOS 设备上成功运行
- 登录功能正常工作
- 没有 Signal 11 崩溃

**日期:** 第 8 天

### 里程碑 4: 生产就绪
- 安全性增强完成
- 性能优化完成
- 文档完整
- 部署流程验证

**日期:** 第 12 天

## 风险管理

### 高风险项

1. **ohos-openssl 不可用或有严重 bug**
   - **概率:** 中
   - **影响:** 高
   - **缓解:** 提前测试基本功能，准备备用方案

2. **OpenSSL 链接失败**
   - **概率:** 中
   - **影响:** 高
   - **缓解:** 详细的构建脚本错误检查，多个配置选项

3. **性能不达标**
   - **概率:** 低
   - **影响:** 中
   - **缓解:** 早期性能测试，优化策略

### 中风险项

1. **测试覆盖不足**
   - **概率:** 中
   - **影响:** 中
   - **缓解:** 制定详细的测试计划，代码审查

2. **文档不完整**
   - **概率:** 中
   - **影响:** 中
   - **缓解:** 边开发边写文档，用户反馈

### 低风险项

1. **向后兼容性问题**
   - **概率:** 低
   - **影响:** 低
   - **缓解:** 保持现有实现不变，充分测试

## 资源需求

### 人力资源
- 后端开发工程师: 1 人
- 测试工程师: 0.5 人（兼职）
- 文档工程师: 0.5 人（兼职）

### 硬件资源
- HarmonyOS 测试设备: 1 台
- 开发机器: 1 台（Windows + HarmonyOS SDK）

### 软件资源
- Rust 工具链
- HarmonyOS SDK
- ohos-openssl 库
- 测试工具

## 验收标准

### 功能验收
- [ ] 在 HarmonyOS 设备上成功登录
- [ ] JWT token 正确生成和验证
- [ ] 所有 API 端点正常工作
- [ ] 没有 Signal 11 崩溃
- [ ] 在其他平台保持正常工作

### 性能验收
- [ ] JWT 创建 < 10ms
- [ ] JWT 验证 < 5ms
- [ ] 支持 100 并发请求/秒
- [ ] 内存使用合理

### 安全验收
- [ ] 使用强密钥
- [ ] 签名验证正确
- [ ] 过期检查正确
- [ ] 没有明显的安全漏洞

### 文档验收
- [ ] 部署文档完整
- [ ] 技术文档准确
- [ ] 故障排查指南有效
- [ ] 代码注释充分

## 后续工作

### 短期（1-2 周）
- 解决数据库写入问题
- 性能进一步优化
- 收集用户反馈

### 中期（1-2 月）
- 实现 token 刷新机制
- 添加 token 黑名单
- 实现多设备登录管理

### 长期（3-6 月）
- 研究 HarmonyOS 原生加密 API
- 考虑外部认证服务集成
- 支持更多认证方式（OAuth, SAML）

## 参考资料

- [ohos-openssl GitHub](https://github.com/ohos-rs/ohos-openssl)
- [Rust OpenHarmony 文档](https://doc.rust-lang.org/rustc/platform-support/openharmony.html)
- [OpenSSL Rust 绑定](https://docs.rs/openssl/latest/openssl/)
- [JWT 标准 RFC 7519](https://datatracker.ietf.org/doc/html/rfc7519)
- [HMAC RFC 2104](https://datatracker.ietf.org/doc/html/rfc2104)
