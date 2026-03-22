# HarmonyOS JWT/OpenSSL 认证系统需求文档

## 介绍

本文档定义了在 HarmonyOS (OpenHarmony) 平台上实现安全 JWT 认证的完整解决方案。由于 HarmonyOS 使用 MUSL libc 且任何纯 Rust 加密库都会导致 Signal 11 崩溃，本方案采用 OpenSSL 库（通过 ohos-openssl 项目）来实现 JWT 签名和验证功能。

## 背景

### 当前问题

1. **Signal 11 崩溃**: 所有纯 Rust 加密库（jsonwebtoken、jwt-simple、hmac+sha2、base64）在 HarmonyOS 上都会导致 SIGSEGV
2. **临时方案不安全**: 当前使用简单字符串校验和的临时方案不适合生产环境
3. **数据库写入问题**: SQLite 写操作也会导致 Signal 11（已通过跳过 last_logon 更新临时解决）

### 解决方案

使用 HarmonyOS 官方支持的 OpenSSL 库（ohos-openssl）来实现 JWT 功能，这是 Rust 官方文档推荐的方案。

## 术语表

- **HarmonyOS**: 华为开发的开源操作系统（OpenHarmony）
- **MUSL libc**: HarmonyOS 使用的 C 标准库实现
- **ohos-openssl**: 为 HarmonyOS 预编译的 OpenSSL 库
- **JWT**: JSON Web Token，用于身份认证的令牌标准
- **HMAC-SHA256**: 基于哈希的消息认证码，使用 SHA-256 哈希算法
- **Signal 11 (SIGSEGV)**: 段错误，通常由内存访问违规引起
- **Cross-compilation**: 交叉编译，在一个平台上编译另一个平台的可执行文件

## 需求

### 需求 1: ohos-openssl 集成

**用户故事:** 作为开发人员，我需要在项目中集成 ohos-openssl 库，以便在 HarmonyOS 上使用 OpenSSL 加密功能。

#### 验收标准

1. THE System SHALL 克隆 ohos-openssl 仓库到项目的 vendor 目录
2. THE System SHALL 在构建脚本中设置 `AARCH64_UNKNOWN_LINUX_OHOS_OPENSSL_DIR` 环境变量
3. THE System SHALL 在 Cargo.toml 中添加 openssl 依赖，配置正确的 features
4. THE System SHALL 验证 openssl 库能够在 HarmonyOS 上成功链接
5. THE System SHALL 在 .gitignore 中排除 vendor/ohos-openssl 目录（如果需要）
6. THE System SHALL 提供清晰的文档说明如何设置 ohos-openssl

**技术细节:**
```toml
# Cargo.toml
[dependencies]
openssl = { version = "0.10", features = ["vendored"] }
subtle = "2.5"  # 常量时间比较，防止 timing attack

[target.'cfg(target_env = "ohos")'.dependencies]
openssl = { version = "0.10" }
```

```powershell
# scripts/build-backend.ps1
$env:AARCH64_UNKNOWN_LINUX_OHOS_OPENSSL_DIR = "$PSScriptRoot\..\vendor\ohos-openssl\prelude\arm64-v8a"
```

### 需求 2: OpenSSL-based JWT 实现

**用户故事:** 作为系统架构师，我需要使用 OpenSSL 实现 JWT 的签名和验证功能，替代当前的临时方案。

#### 验收标准

1. THE System SHALL 使用 OpenSSL 的 HMAC-SHA256 算法进行 JWT 签名
2. THE System SHALL 实现标准的 JWT 结构：header.payload.signature
3. THE System SHALL 支持自定义 claims，包括 user_id、username、token_id、exp
4. THE System SHALL 在 HarmonyOS 环境下自动使用 OpenSSL 实现
5. THE System SHALL 在非 HarmonyOS 环境下保持使用现有的 jwt-simple 实现
6. THE System SHALL 确保 token 过期时间可配置（默认 24 小时）
7. THE System SHALL 提供清晰的错误信息，区分签名错误和过期错误

**技术细节:**
```rust
// src/shared/security/jwt_openssl.rs
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;

pub fn create_jwt_with_openssl(payload: AuthPayload) -> Result<AuthBody, String> {
    // 1. 创建 header (Base64URL)
    let header = json!({
        "alg": "HS256",
        "typ": "JWT"
    });
    
    // 2. 创建 payload (Base64URL)
    let claims = Claims {
        user_id: payload.id,
        username: payload.name,
        exp: (Local::now() + Duration::hours(24)).timestamp(),
        token_id: uuid::Uuid::new_v4().to_string(),
    };
    
    // 3. 使用 OpenSSL HMAC-SHA256 签名
    let signature = sign_with_openssl(&header, &claims, secret)?;
    
    // 4. 组合 token
    let token = format!("{}.{}.{}", header_b64, payload_b64, signature);
    
    Ok(AuthBody::new(token, claims.exp, 86400))
}
```

### 需求 3: 运行时环境检测

**用户故事:** 作为系统开发者，我需要系统能够自动检测运行环境，在 HarmonyOS 上使用 OpenSSL，在其他平台上使用纯 Rust 实现。

#### 验收标准

1. THE System SHALL 检测 `HARMONYOS_MODE` 环境变量来判断是否在 HarmonyOS 上运行
2. THE System SHALL 在 HarmonyOS 环境下使用 OpenSSL-based JWT 实现
3. THE System SHALL 在非 HarmonyOS 环境下使用 jwt-simple 实现
4. THE System SHALL 在日志中清晰标识使用的 JWT 实现方式
5. THE System SHALL 确保两种实现生成的 token 格式兼容（都是标准 JWT）
6. THE System SHALL 支持通过编译时 feature flag 强制选择实现方式

**技术细节:**
```rust
// src/shared/security/jwt.rs
fn is_harmonyos() -> bool {
    std::env::var("HARMONYOS_MODE").is_ok()
}

pub fn create_jwt(payload: AuthPayload) -> Result<AuthBody, String> {
    if is_harmonyos() {
        tracing::info!("🔐 Using OpenSSL-based JWT for HarmonyOS");
        jwt_openssl::create_jwt_with_openssl(payload)
    } else {
        tracing::debug!("🔐 Using jwt-simple for standard platforms");
        jwt_simple::create_jwt_with_simple(payload)
    }
}
```

### 需求 4: 构建脚本增强

**用户故事:** 作为构建工程师，我需要构建脚本能够正确配置 OpenSSL 环境变量，确保编译成功。

#### 验收标准

1. THE Build Script SHALL 检查 ohos-openssl 目录是否存在
2. THE Build Script SHALL 设置 `AARCH64_UNKNOWN_LINUX_OHOS_OPENSSL_DIR` 环境变量
3. THE Build Script SHALL 在编译前验证 OpenSSL 库文件存在
4. THE Build Script SHALL 提供清晰的错误信息，如果 ohos-openssl 未安装
5. THE Build Script SHALL 支持自动下载和设置 ohos-openssl（可选）
6. THE Build Script SHALL 在编译日志中显示 OpenSSL 配置信息

**技术细节:**
```powershell
# scripts/build-backend.ps1

# 检查 ohos-openssl
$opensslDir = "$PSScriptRoot\..\vendor\ohos-openssl\prelude\arm64-v8a"
if (-not (Test-Path $opensslDir)) {
    Write-Host "Error: ohos-openssl not found at: $opensslDir" -ForegroundColor Red
    Write-Host "Please clone: git clone https://github.com/ohos-rs/ohos-openssl vendor/ohos-openssl" -ForegroundColor Yellow
    exit 1
}

# 设置 OpenSSL 环境变量
$env:AARCH64_UNKNOWN_LINUX_OHOS_OPENSSL_DIR = $opensslDir
Write-Host "OpenSSL: $opensslDir" -ForegroundColor Yellow
```

### 需求 5: Token 验证和错误处理

**用户故事:** 作为 API 开发者，我需要系统能够正确验证 JWT token，并提供清晰的错误信息。

#### 验收标准

1. THE System SHALL 验证 JWT token 的签名是否正确
2. THE System SHALL 检查 token 是否过期
3. THE System SHALL 验证 token 的格式是否符合标准
4. THE System SHALL 提供详细的验证失败原因（签名错误、过期、格式错误）
5. THE System SHALL 在验证失败时返回 401 Unauthorized 状态码
6. THE System SHALL 记录验证失败的日志，包含失败原因和 token 部分信息（不记录完整 token）
7. THE System SHALL 支持 token 刷新机制（可选）

**技术细节:**
```rust
pub fn validate_jwt(token: &str) -> Result<Claims, String> {
    if is_harmonyos() {
        jwt_openssl::verify_jwt_with_openssl(token)
    } else {
        jwt_simple::verify_jwt_with_simple(token)
    }
}

pub fn verify_jwt_with_openssl(token: &str) -> Result<Claims, String> {
    // 1. 分割 token
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid token format".to_string());
    }
    
    // 2. 验证签名
    let expected_sig = sign_with_openssl(parts[0], parts[1], secret)?;
    if parts[2] != expected_sig {
        return Err("Invalid signature".to_string());
    }
    
    // 3. 解析 payload
    let claims: Claims = decode_base64_json(parts[1])?;
    
    // 4. 检查过期
    if claims.exp < Local::now().timestamp() {
        return Err("Token expired".to_string());
    }
    
    Ok(claims)
}
```

### 需求 6: 安全性增强

**用户故事:** 作为安全工程师，我需要确保 JWT 实现符合安全最佳实践，防止常见的安全漏洞。

#### 验收标准

1. THE System SHALL 使用强密钥（至少 256 位）进行 HMAC-SHA256 签名
2. THE System SHALL 从环境变量或配置文件读取密钥，不硬编码在代码中
3. THE System SHALL 在生产环境中强制使用强密钥（不允许默认密钥）
4. THE System SHALL 防止时序攻击（使用常量时间比较）
5. THE System SHALL 限制 token 的有效期（默认 24 小时，最长 7 天）
6. THE System SHALL 支持 token 黑名单机制（用于强制登出）
7. THE System SHALL 在日志中不记录完整的 token 或密钥信息

**技术细节:**
```rust
// 从环境变量或配置读取密钥
fn get_jwt_secret() -> String {
    std::env::var("JWT_SECRET")
        .or_else(|_| {
            // 从配置文件读取
            let config = load_config()?;
            Ok(config.jwt_secret)
        })
        .unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                "dev-secret-key-123456".to_string()
            } else {
                panic!("JWT_SECRET must be set in production");
            }
        })
}

// 常量时间比较
// 使用 subtle::ConstantTimeEq 确保比较时间是常量，不泄露长度信息
// ct_eq 内部实现为常量时间，长度不同时直接返回 false，无 timing leak
fn constant_time_compare(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    a.as_bytes().ct_eq(b.as_bytes()).into()
}
```

### 需求 7: 测试和验证

**用户故事:** 作为 QA 工程师，我需要完整的测试套件来验证 JWT 功能在 HarmonyOS 上正常工作。

#### 验收标准

1. THE System SHALL 提供单元测试，验证 JWT 创建和验证功能
2. THE System SHALL 提供集成测试，验证登录流程端到端工作
3. THE System SHALL 提供 HarmonyOS 设备上的实际测试脚本
4. THE System SHALL 测试 token 过期场景
5. THE System SHALL 测试无效签名场景
6. THE System SHALL 测试格式错误的 token 场景
7. THE System SHALL 提供性能测试，确保 JWT 操作不影响系统性能

**技术细节:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_verify_jwt() {
        let payload = AuthPayload {
            id: "user-001".to_string(),
            name: "test_user".to_string(),
        };
        
        let auth_body = create_jwt(payload).unwrap();
        let claims = validate_jwt(&auth_body.token).unwrap();
        
        assert_eq!(claims.user_id, "user-001");
        assert_eq!(claims.username, "test_user");
    }
    
    #[test]
    fn test_expired_token() {
        // 创建已过期的 token
        let expired_token = create_expired_token();
        let result = validate_jwt(&expired_token);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expired"));
    }
    
    #[test]
    fn test_invalid_signature() {
        let token = "header.payload.invalid_signature";
        let result = validate_jwt(token);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signature"));
    }
}
```

### 需求 8: 文档和部署指南

**用户故事:** 作为运维工程师，我需要清晰的文档说明如何在 HarmonyOS 上部署和配置 JWT 认证系统。

#### 验收标准

1. THE System SHALL 提供完整的部署文档，包含所有步骤
2. THE System SHALL 提供 ohos-openssl 安装指南
3. THE System SHALL 提供环境变量配置说明
4. THE System SHALL 提供故障排查指南
5. THE System SHALL 提供性能调优建议
6. THE System SHALL 提供安全配置最佳实践
7. THE System SHALL 更新 HARMONYOS_DEPLOYMENT.md 文档

**文档结构:**
```markdown
# HarmonyOS JWT 部署指南

## 前置条件
- HarmonyOS SDK
- Rust 工具链
- ohos-openssl 库

## 安装步骤
1. 克隆 ohos-openssl
2. 配置环境变量
3. 编译项目
4. 部署到设备
5. 配置 JWT 密钥

## 验证
- 测试登录功能
- 验证 token 生成
- 测试 token 验证

## 故障排查
- Signal 11 问题
- 链接错误
- 运行时错误
```

### 需求 9: 向后兼容性

**用户故事:** 作为系统维护者，我需要确保新的 OpenSSL 实现与现有系统兼容，不影响其他平台的运行。

#### 验收标准

1. THE System SHALL 在非 HarmonyOS 平台上保持使用 jwt-simple 实现
2. THE System SHALL 确保两种实现生成的 token 可以互相验证（如果使用相同密钥）
3. THE System SHALL 不修改 API 接口，保持对外接口不变
4. THE System SHALL 不影响现有的登录、登出、token 验证流程
5. THE System SHALL 支持平滑迁移，允许新旧 token 共存
6. THE System SHALL 提供迁移脚本（如果需要）

### 需求 10: 性能优化

**用户故事:** 作为性能工程师，我需要确保 OpenSSL-based JWT 实现不会显著影响系统性能。

#### 验收标准

1. THE System SHALL 确保 JWT 创建操作在 10ms 内完成
2. THE System SHALL 确保 JWT 验证操作在 5ms 内完成
3. THE System SHALL 使用连接池或缓存优化 OpenSSL 调用
4. THE System SHALL 避免在热路径上进行不必要的内存分配
5. THE System SHALL 提供性能监控指标
6. THE System SHALL 在高并发场景下保持稳定性能

**性能基准:**
```rust
#[bench]
fn bench_jwt_creation(b: &mut Bencher) {
    let payload = AuthPayload {
        id: "user-001".to_string(),
        name: "test_user".to_string(),
    };
    
    b.iter(|| {
        create_jwt(payload.clone()).unwrap()
    });
}

#[bench]
fn bench_jwt_verification(b: &mut Bencher) {
    let token = create_test_token();
    
    b.iter(|| {
        validate_jwt(&token).unwrap()
    });
}
```

### 需求 11: 数据库写入问题解决

**用户故事:** 作为系统开发者，我需要解决 HarmonyOS 上 SQLite 写入导致 Signal 11 的问题。

#### 验收标准

1. THE System SHALL 调查 SQLite 写入崩溃的根本原因
2. THE System SHALL 测试不同的 SQLite 配置选项
3. THE System SHALL 考虑使用 HarmonyOS 原生数据库 API（如果可用）
4. THE System SHALL 如果无法解决，提供替代方案（如只读模式）
5. THE System SHALL 在文档中说明数据库限制
6. THE System SHALL 提供数据同步方案（如果使用只读模式）

**调查方向:**
- SQLite 编译选项（bundled vs system）
- 文件系统权限和路径
- 内存映射配置
- WAL 模式 vs DELETE 模式
- HarmonyOS 文件系统特性

### 需求 12: 监控和日志

**用户故事:** 作为运维人员，我需要详细的日志和监控信息，以便排查 JWT 相关问题。

#### 验收标准

1. THE System SHALL 记录所有 JWT 创建操作，包含用户 ID 和时间戳
2. THE System SHALL 记录所有 JWT 验证失败，包含失败原因
3. THE System SHALL 提供 JWT 操作的统计指标（成功率、延迟）
4. THE System SHALL 在 HarmonyOS 环境下标识使用的 JWT 实现
5. THE System SHALL 记录 OpenSSL 库的版本信息
6. THE System SHALL 提供日志级别配置，支持调试模式

**日志示例:**
```
[INFO] 🔐 JWT: Using OpenSSL-based implementation for HarmonyOS
[INFO] 🔐 JWT: OpenSSL version: OpenSSL 1.1.1 (ohos-openssl)
[DEBUG] 🔐 JWT: Creating token for user: admin-user-001
[DEBUG] 🔐 JWT: Token created successfully, expires at: 2026-01-22T10:30:00Z
[WARN] 🔐 JWT: Token validation failed: Invalid signature
[ERROR] 🔐 JWT: OpenSSL error: ...
```

## 实施优先级

### P0 (必须实现)
- 需求 1: ohos-openssl 集成
- 需求 2: OpenSSL-based JWT 实现
- 需求 3: 运行时环境检测
- 需求 4: 构建脚本增强
- 需求 5: Token 验证和错误处理

### P1 (高优先级)
- 需求 6: 安全性增强
- 需求 7: 测试和验证
- 需求 8: 文档和部署指南
- 需求 9: 向后兼容性

### P2 (中优先级)
- 需求 10: 性能优化
- 需求 12: 监控和日志

### P3 (低优先级)
- 需求 11: 数据库写入问题解决（独立问题，可后续处理）

## 成功标准

项目成功的标准：

1. ✅ 在 HarmonyOS 设备上成功编译和运行
2. ✅ 登录功能正常工作，不出现 Signal 11 崩溃
3. ✅ JWT token 能够正确创建和验证
4. ✅ 在其他平台（Linux、Windows）上保持正常工作
5. ✅ 通过所有单元测试和集成测试
6. ✅ 性能满足要求（JWT 操作 < 10ms）
7. ✅ 文档完整，部署流程清晰

## 风险和缓解措施

### 风险 1: ohos-openssl 不可用或有 bug
**缓解措施:** 
- 提前测试 ohos-openssl 的基本功能
- 准备备用方案（HarmonyOS 原生 API）
- 保留当前的简单 token 方案作为最后的 fallback

### 风险 2: OpenSSL 链接失败
**缓解措施:**
- 详细的构建脚本错误检查
- 提供多个 OpenSSL 路径配置选项
- 文档中提供详细的故障排查步骤

### 风险 3: 性能不达标
**缓解措施:**
- 早期进行性能测试
- 使用缓存和连接池优化
- 考虑使用更轻量的加密算法（如 HMAC-SHA1）

### 风险 4: 数据库问题无法解决
**缓解措施:**
- 将数据库问题与 JWT 问题分离
- JWT 功能不依赖数据库写入
- 提供只读模式或外部数据同步方案

## 相关文件

### 需要修改的文件
- `src/shared/security/jwt.rs` - 主 JWT 模块
- `src/shared/security/jwt_openssl.rs` - 新增 OpenSSL 实现
- `Cargo.toml` - 添加 openssl 依赖
- `scripts/build-backend.ps1` - 增强构建脚本
- `.cargo/config.toml` - 可能需要调整链接配置

### 需要创建的文件
- `src/shared/security/jwt_openssl.rs` - OpenSSL JWT 实现
- `tests/jwt_harmonyos_test.rs` - HarmonyOS JWT 测试
- `docs/HARMONYOS_JWT_OPENSSL.md` - 详细技术文档

### 需要更新的文档
- `HARMONYOS_DEPLOYMENT.md` - 部署指南
- `HARMONYOS_JWT_SOLUTION.md` - 解决方案文档
- `README.md` - 项目说明

## 参考资料

- ohos-openssl: https://github.com/ohos-rs/ohos-openssl
- Rust OpenHarmony 文档: https://doc.rust-lang.org/rustc/platform-support/openharmony.html
- OpenSSL Rust 绑定: https://docs.rs/openssl/latest/openssl/
- JWT 标准: https://datatracker.ietf.org/doc/html/rfc7519
- HMAC-SHA256: https://datatracker.ietf.org/doc/html/rfc2104
