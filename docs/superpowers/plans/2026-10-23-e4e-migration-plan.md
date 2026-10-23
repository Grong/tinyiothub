# E4e 存量驱动迁移计划：ABI v2 硬切

- 日期：2026-10-23
- 分支：`ai-consolidation`
- 状态：**计划**。E4e 只交付本文与 `driver-check inventory`；实际迁移执行（改代码、重建 `.so`、维护窗口切换）**可另立阶段**，见 §8。
- 上位文档：
  - `docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md` §5.2
  - `docs/superpowers/specs/2026-10-23-e4e-acceptance-suite-design.md` §4
- 已裁定原则：v1 不兼容、不做双 ABI 导出；Hub 与驱动在**同一维护窗口原子替换**；v1 `.so` 由新 Hub 直接拒载并给出迁移指引。

## 0. 结论摘要

| 对象 | 现状 | 迁移动作 | 门禁 |
|---|---|---|---|
| 内置驱动（native） | E4a 已迁 ABI v2 + manifest + `describe()` | 无需改代码；每版本复跑验收 | `driver-check inventory` 内置清单 + demo 全套 |
| `drivers/*`（7 个 cdylib 包） | **2026-10-23 盘点：均为空壳**（`driver_init` 桩或仅注释），无 manifest/无 v2；已退役 | 不再迁移；未来真实协议驱动按 §3 新建，模板=`drivers/demo-declarative` | 新包 `driver-check --profile all` |
| 外部已部署 v1 `.so` | 由旧 SDK 构建；安装布局见 §1.3 | 盘点 → v2 SDK 重建 → staging 验收 → 维护窗口原子替换 Hub+驱动 | `inventory --strict` = 0 问题 + 设备在线/read/tools |
| 数据（声明/模板/zone/position_ingest） | E4a/E4d 迁移为 additive | 重绑重物化、drift 报告、zone 切换、摄取门回填 | 验收 `migration`/`materialization`/`placement` + 数据核对 |
| 回滚 | — | 整包回退旧 Hub + 旧驱动；DB 不回滚（additive） | 回滚演练记录 |

硬切含义：维护窗口内 **Hub 与全部插件同时替换为新版本**。旧 Hub 不能加载 v2 导出、新 Hub 不能加载 v1 导出，任何混搭都会导致该 workspace 的驱动不可用；不做兼容层。

## 1. 存量盘点

盘点以 `driver-check inventory` 为唯一机器可读入口（§6.1）。当前基线（2026-10-23，`ai-consolidation`）：

### 1.1 内置驱动（native，编译进 Hub）

| driver | version | 实现位置 | manifest | ABI | 双层声明 |
|---|---|---|---|---|---|
| `simulator` | `2.0.0` | `crates/runtime/src/driver/drivers/simulated_driver.rs` | `crates/runtime/manifests/simulator.json` | v2 | `describe()` → `declaration_from_thing` |
| `modbus_rtu` | `1.0.0` | `crates/runtime/src/driver/drivers/modbus_driver.rs` | `crates/runtime/manifests/modbus_rtu.json` | v2 | `driver_options` 点位 → `describe(config)` |
| `snmp` | `1.0.0` | `crates/runtime/src/driver/drivers/snmp_driver.rs` | `crates/runtime/manifests/snmp.json` | v2 | `driver_options` 点位 → `describe(config)` |

说明：

- 上位文档写作 `simulated`，代码/清单实际 driver 名为 **`simulator`**；盘点与安装校验以代码为准。
- 三个内置 manifest 均为 `dynamicCapabilities: true`、`abiVersion: 2`，`inventory` 应报告 `errors=0`。
- 内置实现不可被 workspace 包遮蔽：`AccessService::install_dir` 按 `is_driver_supported` 拒绝内置名。未来新建协议驱动必须使用与内置不同的 driver 名（见 §3.2）。

### 1.2 `drivers/*`（动态驱动源码，2026-10-23 退役）

盘点发现 7 个包**没有可迁移实现**：

| 包 | 实际内容 |
|---|---|
| `drivers/mqtt` / `modbus` / `onvif` / `snmp` | 单个 `driver_init() -> i32` 桩，无 vtable/读写/manifest |
| `drivers/bacnet` / `opcua` / `mqtt-bridge` | 仅一行模块注释 |

因此它们已从仓库退役（`git rm`）；真实存量驱动只有 3 个内置（§1.1），已在 E4a 迁到 v2 + manifest。`drivers/demo-declarative` 保留为 workspace 内的参考驱动/模板。未来实现真实协议驱动时按 §3 新建，而不是复活空壳。

另外两个仓内 v1 参考物（不在 workspace，无 manifest）：

- `examples/bacnet-driver`：`edition 2021`，依赖 `../../sdks/driver-sdk`（路径当前不存在），旧版 `ThingDriver` 签名。
- `examples/example-plugin`：依赖旧 `iot_edge_driver_sdk`，`export_driver!` v1 宏。

二者按外部 v1 产物处理：重建或删除，不得作为 v2 示例发布。

### 1.3 外部已部署 v1 `.so`（运行时安装布局）

Hub 的规范安装位置（`DriverLoader` / `AccessService::install_dir`）：

```
<data-dir>/drivers/workspaces/<workspace>/<driver>/<version>/
  driver.manifest.json
  driver.so | driver.dylib | driver.dll        # find_binary 候选
```

- 安装成功会在 `driver_declarations` 写入 `status='active'` 行（workspace/driver/version + manifest/binary hash）。
- 重装以 `.backup-<version>-<nanos>` 旁路目录做原子回滚；`inventory` 忽略点号目录。
- v1 `.so` 的判定：加载成功但缺少 `tinyiothub_driver_vtable_v2` 导出 → inventory 分类 `v1/unsupported`，错误文案含 `ABI v1 is unsupported by E4. Migrate to ABI v2 (async read/execute + cancel + describe)`。
- 无法加载 → `invalid`；无二进制 → `none`；manifest 不可读 → `unreadable`；目录名与 manifest identity 不一致 → 额外输出 `pathIdentity`。
- **inventory 只扫描规范目录**。若历史部署把 `.so` 放在别处，或 `driver_declarations` 行指向缺失目录，静态盘点会漏报，见 §7.1。

### 1.4 盘点基线采集

```bash
# 内置 + 规范安装目录（信息性；有问题仍 exit 0）
cargo run -p driver-check -- inventory --data-dir /var/lib/tinyiothub

# CI/迁移门禁：v1/invalid/unreadable/内置清单错误 -> exit 1
cargo run -p driver-check -- inventory --data-dir /var/lib/tinyiothub --strict
```

将 JSON 存档为迁移工单附件；`--strict` 的退出码作为「是否仍有 v1 残留」的机器判据。data-dir 不存在/不可读是调用错误（exit 2），避免空盘点假通过。

## 2. 内置驱动（E4a 已完成）

1. **代码状态**：三个 native 驱动已实现 v2 trait（async read/execute + cancel token + `describe()`），清单随二进制编译（`builtin_manifest_jsons()`）。
2. **双层声明**：`simulator` 由 thing 字段推导；`modbus_rtu`/`snmp` 从 `driver_options` 点位生成设备声明，实例声明与包级 manifest 分离。
3. **迁移动作**：无。升级 Hub 即完成；无需保管/替换内置 `.so`。
4. **每版本复核**：
   - `driver-check inventory --data-dir <dir>` 内置三项 `errors=0`；
   - `driver-check --package drivers/demo-declarative --profile all` 通过（契约回归）；
   - 对每个内置驱动绑定一个真实物，抽查 `GET /v1/things/{id}/declaration` 与投影属性/命令；
   - `migration` profile 的 `v1.rejected`/`v2.version` 为 pass。
5. **风险**：内置名与 `drivers/*` 重名会被安装拒绝（§3.2），属于预期 fail-closed。

## 3. 未来协议驱动开发（原 `drivers/*` 迁移，另立阶段）

目标：未来新增真实协议驱动时，按 `drivers/demo-declarative` 模板进入 workspace 编译门禁，具备 manifest + ABI v2（async/cancel/describe）并通过逐包验收。空壳已退役，不再按 7 个包批量迁移。

### 3.1 新建一个协议驱动包（一次一个，独立提交）

1. 从 `drivers/demo-declarative` 复制结构，新建 `drivers/<protocol>/` 并加入根 `Cargo.toml` 的 `members`（一次只加一个包）。
2. 修正各包 lint/依赖，使 `cargo check --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --lib --bins --tests` 通过。
3. CI 已有 workspace build/test；新包合并后必须通过 `cargo build -p <pkg>` 与 §3.4 的逐包验收。未完成的协议不要提前合并空壳。

### 3.2 每包迁移模板（以 `drivers/demo-declarative` 为准）

1. **manifest**：`driver.manifest.json`，`manifestVersion=1.0`、`identity.abiVersion=2`、`dynamicCapabilities` 按协议选择；声明 config schema、tools、stateFields、sources、resources、placement、limits。
2. **driver 名**：必须全局唯一且不能与内置（`simulator`/`modbus_rtu`/`snmp`）重名；`drivers/modbus` 建议 `modbus`（TCP/串口参数在 config），不得复用 `modbus_rtu`。
3. **ABI v2 导出**：

```rust
#[unsafe(no_mangle)]
pub extern "C" fn tinyiothub_driver_vtable_v2() -> *const DriverVTableV2 { /* ... */ }
```

   `read`/`execute` 异步化并接受 cancel token；`stop` 后 ≤1s 在途取消（否则 `cancelled_unknown` 留痕）；`describe(config)` 遵守 2s/256 KiB 限制。
4. **双层声明**：动态能力驱动由 `describe(config)` 输出设备声明；固定能力驱动可省略 describe（由 manifest 物化）。模板能力仅在动态驱动下作为 bootstrap。
5. **单元/协议测试**：协议编解码、超时/cancel、非法参数拒绝、describe 超时/上限；无硬件时用本地 mock server 或录制的报文，不假装 PASS。
6. **打包产物**：`driver.manifest.json` + `driver.<ext>` 放入规范目录（§1.3）后再跑验收；仓内源码通过 `cargo build` 得到 cdylib。

### 3.3 未来协议驱动优先级（不是存量迁移）

| 顺序 | 包 | 理由 | 前置 |
|---|---|---|---|
| 1 | `mqtt` | 使用面最广、协议最简单（poll/push 明确），先验证 manifest + source/demand 模型 | 模板复制 |
| 2 | `modbus` | 与内置 `modbus_rtu` 共享协议知识，可复用字段映射；注意 driver 名不能叫 `modbus_rtu` | mqtt 模板 |
| 3 | `bacnet` | 读点模型成熟；需对象/属性映射 | 同上 |
| 4 | `opcua` | 会话/订阅语义较重，需显式 cancel | 同上 |
| 5 | `snmp` | 与内置 snmp 对齐 OID 映射 | 同上 |
| 6 | `onvif` | 依赖 `vendor/onvif-rs`（独立 workspace）与媒体工具，构建最重 | 前 5 包契约稳定 |
| 7 | `mqtt-bridge` | 跨协议桥接，依赖 mqtt 与目标协议均已迁移 | mqtt + 对端包 |

每完成一包：提交 `feat(driver-<name>): 迁移 ABI v2 + manifest`，附 §6.2 的验收 JSON。

### 3.4 逐包验收门禁（每包必须通过）

```bash
cargo build -p tinyiothub-driver-<name>
# 将 manifest + cdylib 复制到临时包目录或 staging data-dir 后：
cargo run -p driver-check -- --package <staged-package-dir> --profile all
```

- 10 个自动 profile 无 `fail`；`warn` 需在报告说明；`skip` 必须带「该包无此能力/无硬件」证据。
- `inventory --data-dir <staging> --strict` 对该包输出 `v2` 且 exit 0。
- 报告 JSON 存入迁移工单；若经 Hub 运行，使用 `POST /v1/drivers/{driver}/acceptance`（admin）写入 `acceptance_reports`。

## 4. 外部已部署 `.so` 迁移（v2 硬切）

### 4.1 盘点阶段（不改线上）

1. 冻结驱动/Hub 变更窗口（只读或变更冻结）。
2. 对每个环境执行 `inventory --strict`，导出 JSON；按 workspace 建立清单：`v2` / `v1/unsupported` / `invalid` / `none` / `unreadable`。
3. 交叉核对 `driver_declarations` 中 `status='active'` 的 `(workspace, driver, version)` 与磁盘条目；缺失目录、非规范布局、手工拷贝的 `.so` 单独列出（§7.1）。
4. 通知包所有者：v1 在新 Hub 不可用，必须重建；登记协议、负责人、目标版本、是否需要硬件。

### 4.2 重建阶段

1. 使用 v2 SDK/模板（参考 `drivers/demo-declarative`）重写导出：`tinyiothub_driver_vtable_v2`、async read/execute、cancel、`describe`。
2. 保留原 driver 名与语义；版本号递增（如 `1.x → 2.0.0`），manifest `abiVersion=2`。
3. 产物 = `driver.manifest.json` + `driver.<ext>`；记录 manifest/binary SHA-256。
4. 在 staging data-dir 的规范目录安装，跑：
   - `driver-check --package <staged-package-dir> --profile all`
   - `inventory --data-dir <staging> --strict`（目标 0 问题）
   - 绑定一个真实/影子设备：在线、read、工具调用、stop/cancel。

### 4.3 维护窗口切换（原子替换）

严格顺序，禁止只换 Hub 或只换驱动：

1. **停止 Hub**（确认在途命令/工具已排空或超时）。
2. **备份**：整个 `<data-dir>/drivers/workspaces`、`driver.manifest.json` 集合、数据库文件（或快照），以及旧 Hub 二进制；记录版本号。
3. **准备新发布集**：同一构建产物中包含新 Hub + 全部 v2 驱动包；校验哈希。
4. **原子替换**：
   - 推荐目录级替换：把新驱动树放到 `<data-dir>/drivers/workspaces.new`，`rename` 旧树为 `.rollback-<ts>`，再 `rename` 新树为正式名（同一文件系统、单次 rename 原子）；
   - 或逐包替换 `driver.manifest.json` + 二进制（同一版本目录内先写临时文件再 `rename`）；
   - 替换 Hub 二进制（`rename` 覆盖）。
5. **启动 Hub**；观察日志：不应出现 `ABI v1 is unsupported`、`failed to rehydrate installed driver package`。
6. **验证**：
   - `inventory --strict` → 无 `v1/unsupported`/`invalid`/`unreadable`，exit 0；
   - 每 workspace 包列表与 `driver_declarations` active 行一致；
   - 设备在线、read 有值、工具调用成功、stop 在途 ≤1s cancelled；
   - 每包验收报告（§6.2）归档。
7. **清理由观察期后执行**：删除 `.rollback-<ts>`/`.backup-*`；保留至少一个发布周期。

### 4.4 回滚

触发条件：启动失败、v1 拒载残留未清、设备大面积离线、关键工具失败。

1. 停止新 Hub。
2. 还原旧 Hub 二进制 + 旧驱动树（整目录 `rename` 回去，或从备份复制）。
3. 数据库**不回滚**：E4a/E4d 迁移是 additive（新表/列/索引），旧 Hub 可忽略新表；`acceptance_reports`、`device_declarations` 保留。只有在确认 schema 不兼容时才恢复 DB 快照（需单独评审）。
4. 启动旧 Hub，验证设备在线、read、工具调用。
5. 记录失败原因；修复后重新走 §4.2/§4.3。
6. **演练要求**：staging 至少执行一次「切换 → 回滚 → 再切换」，记录耗时与数据一致性。

## 5. 数据迁移

### 5.1 设备声明与重物化

- 驱动安装/升级成功后，`AccessService::install_dir` 调 `rebind_bound_devices`：对 `(workspace, driver)` 下 `status='active'` 的绑定物逐个 `bind_thing` → `describe(config)` → upsert `device_declarations` → 重物化属性/命令。
- 单个设备重绑失败不会回滚整个安装：保留旧声明并在安装响应 `warnings` 中报告（drift 而非失败）。
- **执行阶段补充**：对存量设备主动触发一次全量重绑（重装同版本包即可走同一路径；若需要不改包的重绑，另加 admin 命令/接口，属执行阶段工具）。
- **Drift 报告**：对比重绑前后的 `device_declarations.declaration_hash` 与投影行，输出新增/变更/消失能力清单；消失的 `source='driver'` 属性/命令由 `mark_stale_declared_capabilities` 标为 `source='stale'`（**保留行，不删除**）。
- 声明缓存按物失效，避免旧声明继续用于调用校验。

### 5.2 模板能力投影 drift

- 创建物时（`apps/cloud/src/domains/thing/service/mod.rs`）：
  - **固定能力驱动**（`CreationMode::Declarative`）：跳过模板属性/命令拷贝，能力完全来自 manifest；
  - **动态能力驱动**（`TemplateBootstrap`）：先拷贝模板行，再由 `describe()` 重物化细化。
- `mark_stale` 只处理 `source='driver'` 行；`source='template'` 行原样保留。因此模板拷贝出的多余能力**不会自动消失**，需在 drift 报告中单独列出，由模板维护者决定更新/停用。
- 场景包实例化：对已声明驱动的物跳过能力拷贝（与上述创建路径一致）。这是行为变更，执行阶段需回写场景模板 spec 并在 staging 对比实例化前后能力集。

### 5.3 maps / `zone_source` 切换

- 迁移 DDL 默认：`workspaces.zone_source = 'hierarchy'`（存量行）。
- `create_workspace` 对新工作区写 `'maps'`；但导入/模板/场景等直接 `INSERT workspaces` 的路径仍走 DEFAULT `'hierarchy'`（E4d 结转）。迁移期语义安全：层级回退可用。
- 切换顺序（按 workspace）：
  1. 保持 `hierarchy`，导入/绘制 maps、places（zone）、校验几何与 bounds；
  2. 为设备补坐标或开启 `position_ingest`（§5.4）；
  3. 验收 `placement` profile / E2E（层级 ↔ maps 双向切换）；
  4. 用 DB 管理入口 `set_zone_source(workspace, 'maps')` 切权威（当前无 HTTP 路由，执行阶段可加 admin 接口或受控脚本）；
  5. 观察 E1b/E3 的快照 zone 字符串；
  6. 回退 = `set_zone_source(..., 'hierarchy')`。
- 未填充 maps 的工作区**不要**切 `maps`：有图但点在 zone 外会得到空 zone；无任何空间数据时 LiveState 才回退 hierarchy。
- 迁移完成后（E5 前）再删除层级回退路径。

### 5.4 `position_ingest` 回填 / 启动重绑

- DDL：`things.position_ingest INTEGER NOT NULL DEFAULT 0`（存量物为 0）。
- 何时置 1：物化时 `runtime` 且 `placement.fromSource` 指向 manifest 中 `kind=position` 的 source。
- 当前缺口：`rehydrate()` 启动只重载驱动包，**不重绑设备**；`rebind_bound_devices` 仅在安装/升级时运行。重启后旧 runtime 设备可能保持 `position_ingest=0`，驱动上报的 `position_*` 被忽略（坐标不更新、zone 为空）。
- 执行阶段动作（二选一或都做）：
  1. **一次性回填**：对每个 active `device_declarations` 重新物化（管理命令或重装包），核对 `position_ingest`；
  2. **启动重绑**：`rehydrate()` 成功后对 active 绑定物执行 `rebind_bound_devices`（幂等；失败保留旧声明并告警）。
- 验证：绑定 runtime + position source 的物，重启 Hub 后 `get_thing_position_ingest=true`，驱动读数写入 x/y/trust；关闭门后静态坐标不被漂移。
- 注意：`fixed` 驱动/无 position source 的物必须保持 0；API 校正坐标不得改变该门（已有测试）。

## 6. Inventory 与验收 gate

### 6.1 `driver-check inventory`

```bash
cargo run -p driver-check -- inventory --data-dir <dir> [--strict]
```

- JSON：`dataDir`、`builtin[]`（driver/version/source/error）、`installed[]`（workspace/driver/version/path/manifestHash/binaryPath/binaryHash/abi/pathIdentity/error）。
- 一行摘要：内置数/错误数、安装包数、各 ABI 分类计数、`OK`/`ATTENTION`。
- 退出码：默认 0（信息性）；`--strict` 在存在 `v1/unsupported`、`invalid`、`unreadable` 或内置清单错误时 1；data-dir 缺失/不可读 2。
- 用途：
  - 迁移前基线；
  - CI 门禁（staging data-dir 快照上跑 `--strict`）；
  - 切换后「无 v1 残留」证明；
  - 与 `driver_declarations` 对账（§4.3 步骤 6）。

### 6.2 验收 gate

统一判据：10 个自动 profile（core、materialization、migration、lifecycle、telemetry、tools、streaming、resources、cancel、placement）无 `fail`；`skip` 必须写明原因；`warn` 需解释。

| Gate | 命令 | 通过标准 |
|---|---|---|
| demo 契约回归 | `cargo build -p demo-declarative && cargo run -p driver-check -- --package drivers/demo-declarative --profile all` | exit 0，10 profiles，`PASS` |
| 单包（迁移后） | 暂存包目录后 `driver-check --package <dir> --profile all` | exit 0，无 fail |
| inventory | `driver-check inventory --data-dir <staging> --strict` | exit 0，`v2` 计数 = 包数 |
| workspace 回归 | `cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --lib --bins --tests` | exit 0 |
| web 回归 | Node 24：`npx tsc --noEmit`、`npx vitest run`、`npm run build` | exit 0 |

报告归档：

- CLI 模式输出「JSON + summary」；JSON 作为 CI artifact 或迁移工单附件。
- Hub 内：`POST /v1/drivers/{driver}/acceptance`（admin）把聚合报告写入 `acceptance_reports`（additive）。
- 逐包报告命名建议：`acceptance-<driver>-<version>-<date>.json`，与 inventory 基线一起归档。

## 7. 风险与缓解

### 7.1 未发现的 v1 产物

- **漏报面**：inventory 只扫规范目录；非规范路径的 `.so`、已删除目录但 `driver_declarations` 仍 active、手工 `lib*.so` 命名、启动时从其他目录加载。
- **缓解**：
  1. 盘点同时跑全盘扫描：`find <data-dir> -name '*.so' -o -name '*.dylib' -o -name '*.dll'`，逐个人工分类；
  2. 对账 `driver_declarations` active 行 vs inventory installed；
  3. 切换后用日志关键字 `ABI v1 is unsupported` / `failed to rehydrate` 做零告警检查；
  4. staging 先复制真实 data-dir 演练；
  5. 观察期保留 `.rollback-*` 与旧 Hub。

### 7.2 部分协议无运行时驱动

- `drivers/*` 目前是桩/注释，**不能**产出真实 PASS；`onvif` 依赖 `vendor/onvif-rs`，构建与硬件成本最高。
- 缓解：按 §3.3 顺序逐包交付；无硬件时用 mock/录制报文做协议与 cancel 测试；验收报告允许 `skip` 但必须有证据与工单；禁止用 demo 的 PASS 代替该包报告。

### 7.3 fmt 基线漂移

- `crates/agent/src/runtime/thing_agent/scheduler.rs`、`crates/core/src/watch.rs` 存在存量 fmt 漂移。迁移提交前执行 `cargo fmt --all` 后**回滚这两个文件的无关 churn**；单独 `style:` 提交清偿，避免混入迁移 diff。

### 7.4 MSRV

- workspace `rust-version = 1.85`，但 E4 代码使用 let-chains（需 1.88+）；CI 固定 1.95.0。迁移构建必须使用固定工具链；执行阶段更新 `rust-version` 或在发布说明中明确。

### 7.5 测试/运行期全局注册表污染

- `DriverRegistry` 是进程级全局；验收/单测必须使用唯一 driver 名并在结束时卸载。迁移后的新包测试沿用该约定，避免跨测试串扰。

### 7.6 原子替换的半失败

- 跨文件系统 `rename` 不原子；磁盘满/权限错误可能留下半套。缓解：同文件系统、先写 `.new` 再 rename、逐文件校验哈希、启动即验证、失败立即执行 §4.4 回滚。
- 禁止热插拔：运行中的旧 Hub 不加载 v2；新 Hub 不接受 v1。

### 7.7 数据漂移与模板行为变更

- 只 upsert + stale，不删除；drift 报告必须人工复核。
- 固定能力驱动跳过模板拷贝是行为变更；场景模板 spec 需回写，staging 对比实例化能力集。
- `position_ingest` 回填前，运行时位置读数会被静默忽略；迁移报告中显式列出受影响物。

## 8. 执行阶段拆分（明确边界）

本计划的 E4e 交付物：

1. 本文档；
2. `driver-check inventory` 子命令与其测试；
3. demo-declarative 全套自动验收 E2E（10 profiles + inventory v2）；
4. 执行记录 `2026-10-23-e4e-execution-record.md`。

**不在 E4e 执行**：

- 修改 `drivers/*` 源码或加入 workspace；
- 重建任何外部 `.so`；
- 维护窗口切换与数据回填；
- `zone_source` admin 接口、启动重绑实现。

执行阶段建议拆为独立计划：`drivers/*` 逐包迁移（§3）→ 外部包重建与 staging 验收（§4.2）→ 数据回填与 zone 切换演练（§5）→ 维护窗口切换 + 回滚演练（§4.3/§4.4）→ 清理由结转。
