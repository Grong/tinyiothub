# shared/ 组件审计清单

> ⚠️ **重要**：复用前必须检查组件状态。标记为 `❌ 勿复用` 的禁止在新代码中使用。

---

## ✅ 可安全复用

| 组件路径 | 用途 | 状态 | 备注 |
|---------|------|------|------|
| `shared/error.rs` | 统一错误类型 | ✅ 可用 | 使用 thiserror，建议继续用 |
| `shared/security/jwt.rs` | JWT 工具 | ✅ 可用 | 标准实现，可复用 |
| `shared/identifier.rs` | ID 生成 | ✅ 可用 | UUID 生成封装 |
| `shared/command.rs` | 命令执行 | ✅ 可用 | 标准子进程封装 |
| `dto/response/builder.rs` | API 响应 | ✅ 可用 | 必须用这个 |
| `infrastructure/config/` | 配置管理 | ✅ 可用 | config-rs 封装 |
| `infrastructure/persistence/database.rs` | 数据库连接 | ✅ 可用 | 连接池封装 |
| `shared/network.rs` | 网络工具 | ✅ 可用 | IP/端口处理 |

---

## ⚠️ 可复用但需检查

| 组件路径 | 用途 | 状态 | 备注 |
|---------|------|------|------|
| `shared/performance.rs` | 性能监控 | ⚠️ 检查 | 确保指标定义一致再复用 |
| `shared/scripting.rs` | 脚本执行 | ⚠️ 检查 | 评估是否需要抽象 |
| `shared/utilities/sn.rs` | 序列号工具 | ⚠️ 检查 | 确认与业务无关再复用 |

---

## ❌ 禁止复用（需要重构）

| 组件路径 | 问题 | 状态 | 替代方案 |
|---------|------|------|---------|
| `shared/utilities/cmd_util.rs` | 疑似重复命令封装 | ❌ 勿复用 | 用 `shared/command.rs` |
| `utils/validation.rs` | 分散的验证逻辑 | ❌ 勿复用 | 统一到 domain 验证 |
| `utils/sql_security.rs` | SQL 注入防护工具 | ❌ 勿复用 | 用 SQLx 参数化查询即可 |
| `utils/trace_util.rs` | 追踪工具 | ❌ 勿复用 | 用 shared/tracing 统一入口 |

---

## 🔍 如何使用

```bash
# 复用前，先查这个清单
# 如果组件标记为 ❌，则：
#   1. 不复用该组件
#   2. 在 shared/ 里创建或完善正确的实现
#   3. 更新本清单

# 搜索可用组件
grep "✅ 可用" SHARED_AUDIT.md
```

---

## 📝 维护规则

- 新增 shared/ 组件必须更新本清单
- 禁止将业务逻辑放入 shared/
- shared/ 只能包含**无业务依赖**的通用工具

---

_本文件由架构检查工具自动维护更新_
