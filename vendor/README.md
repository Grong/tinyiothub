# 第三方库 Fork 说明

本目录包含项目使用的第三方库的本地 fork 版本。

## 为什么需要 Fork？

我们 fork 这些库是因为需要特定的功能、配置或修复，这些修改尚未合并到上游或需要针对项目进行定制。

---

## 📦 库列表

### rusqlite
- **上游仓库**: https://github.com/rusqlite/rusqlite
- **Fork 原因**: 使用 bundled-full 特性，包含完整的 SQLite 库，便于交叉编译
- **主要修改**: 配置调整，使用 bundled 模式
- **版本**: 基于上游稳定版本
- **维护策略**: 定期同步上游更新

### tokio-modbus
- **上游仓库**: https://github.com/slowtec/tokio-modbus
- **Fork 原因**: 需要特定的 Modbus RTU 功能增强和设备兼容性
- **主要修改**: 
  - Modbus RTU 协议优化
  - 设备驱动适配
- **版本**: 基于上游稳定版本
- **维护策略**: 考虑向上游贡献补丁

### onvif-rs
- **上游仓库**: https://github.com/lumeohq/onvif-rs
- **Fork 原因**: 需要支持特定的 ONVIF 设备和协议扩展
- **主要修改**:
  - 支持更多 ONVIF 设备型号
  - 协议扩展和优化
- **版本**: 基于上游稳定版本
- **维护策略**: 定期同步上游更新

---

## 🔧 使用方式

在项目根目录的 `Cargo.toml` 中通过路径依赖引用：

```toml
[dependencies]
# 第三方库 - 本地 fork
rusqlite = { path = "vendor/rusqlite", features = ["bundled-full"] }
tokio-modbus = { path = "vendor/tokio-modbus" }
onvif = { path = "vendor/onvif-rs/onvif" }
```

---

## 🔄 更新策略

### 定期维护
1. **检查上游更新** - 每月检查一次上游仓库的更新
2. **评估兼容性** - 评估上游更改是否影响我们的修改
3. **合并更新** - 必要时合并上游更改
4. **测试验证** - 运行完整测试套件验证兼容性

### 贡献上游
- 对于通用的功能改进，考虑向上游提交 PR
- 保持与上游社区的良好沟通
- 记录我们的修改，便于向上游贡献

### 版本管理
- 使用 Git 子模块或记录上游 commit hash
- 在 CHANGELOG 中记录重要的同步和修改
- 保持与项目主版本的兼容性

---

## 📝 修改记录

### rusqlite
- 2024-12: 初始 fork，配置 bundled-full 特性

### tokio-modbus
- 2024-12: 初始 fork，添加项目特定的 Modbus 驱动支持

### onvif-rs
- 2024-12: 初始 fork，添加特定设备支持

---

## ⚠️ 注意事项

1. **不要直接修改** - 除非必要，避免直接修改这些库的代码
2. **记录修改** - 如果必须修改，请在此文档中记录
3. **保持同步** - 定期检查上游更新，避免版本过时
4. **测试覆盖** - 修改后务必运行测试确保兼容性

---

**维护负责人**: 开发团队  
**最后更新**: 2024年12月22日
