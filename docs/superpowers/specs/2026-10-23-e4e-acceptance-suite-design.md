# E4e 全套验收 + 迁移计划子 spec

- 日期：2026-10-23
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md`（§4.2 验收、§5.2 迁移）
- 范围：E4e，E4 收口。E1-E4d 已交付
- 状态：设计两问已与作者确认（2026-10-23）

## 0. 关键裁定

| # | 主题 | 裁定 |
|---|---|---|
| 1 | Test hub | **真实驱动包 + 内存 Hub**：加载包 `.so` 到真实 DriverRegistry，用内存 ThingCache/EventBus/DemandTracker/DataServer/worker 搭隔离环境；lifecycle/telemetry/tools 对真实驱动执行。streaming 的 demand 门控用确定性 `Sampler` + fake worker 契约测试（无 sleep），另加 `streaming.driver_source`：TestHub 加载真实 `.so`、spawn 真实 worker、读取 `demandOnly` 源并校验其声明字段，补齐真实驱动执行证据 |
| 2 | Manual/交互 | **不做 manual profile、不做 `--interactive`**（显式覆盖上位 spec §4.2 的 AGH 全套 manual 项）；`--no-hardware` 无需实现；所有验收为自动检查 |
| 3 | 范围 | 补齐 lifecycle/telemetry/tools/streaming 四个自动 profile；保留 core/materialization/migration/resources/cancel/placement；全套 `all` 对 demo-declarative 通过 |
| 4 | Streaming 证明 | demo-declarative manifest 增加一个 `demandOnly:true` 的 `slow` source 与一个 `level2` state field（驱动 read 返回两者），使 streaming profile 能验证 demand 起停 + health/telemetry 常开 |
| 5 | 迁移计划 | 交付文档 + `driver-check inventory` 子命令：盘点内置/`drivers/*`/已安装 `.so`/v1 产物，给出硬切、数据迁移、zone_source 切换与回滚 runbook；执行可另立阶段 |

## 1. 组件

```
crates/runtime/src/acceptance/
  test_hub.rs        TestHub（内存 cache/bus/demand/worker + 包加载）
  profiles/lifecycle.rs
  profiles/telemetry.rs
  profiles/tools.rs
  profiles/streaming.rs
  runner.rs          all/具名 profile
crates/driver-check/
  src/main.rs        profiles 列表；inventory 子命令
  tests/e2e.rs       demo 全套
docs/superpowers/plans/2026-10-23-e4e-migration-plan.md
drivers/demo-declarative/  manifest + read 返回 level/level2
```

边界：

1. runtime 零 db；TestHub 全部内存实现（ThingCacheSource/EventBus/MapStore 可复用 fake）。
2. 包加载只加载真实 `.so`；v1 拒载已在 E4a。
3. 测试结束卸载驱动 registry 条目，避免进程级污染。
4. 验收只观察 Hub 公开行为（worker/DataServer/EventBus/LiveState/sampler），不直接读驱动内部。
5. 迁移执行不在 E4e；只交计划与 inventory 工具。

## 2. Profiles

### lifecycle
- spawn worker → read → worker online / Thing.status online。
- stop worker → offline；重新 spawn/read → 恢复 online。
- 失败读不 panic；worker 可继续。

### telemetry
- 一次 read 后 required state fields 均出现；类型/range 符合声明。
- interval 生效：**cadence 归 sampler 所有**——驱动只在 `submit_read` 时读，profile 自身两次 read 的时间差不能证明驱动 interval。由 `access::sampler` 的 `always_source_samples_every_interval` / `interval_is_clamped_to_min_interval` 覆盖；telemetry profile 的 `telemetry.interval` 记 **skip 并注明归属**（不 sleep、不做假声明）。
- push 驱动（若包声明）经 ValueSink 送达；poll 驱动跳过 push 检查（skip）。

### tools
- 声明 `safe` 工具（demo `echo`）：合法参数成功；非法参数拒绝且带声明范围。
- 未声明工具拒绝。
- `control/dangerous` 工具不自动调用（skip/manual 不适用，记 skip）。

### streaming
- `demandOnly` 源（demo `slow`）：无 demand → 不采样（submit_read 不含 slow）；订阅/近期读取 → 采样；撤销 + 30s 宽限 → 停采。
- `health:true`/普通源持续采样；常开源只认 sampler 会 `submit_read` 的 `poll`/`position` 类型（`health` 的 push 源不能证明连续性）；无任何常开采样源时 quiet/stop 记 skip 并注明。
- push/stream 驱动 set_demand 变化（若包声明；否则 skip）。
- `streaming.driver_source`：TestHub 加载真实包 `.so`、spawn 真实 worker、只读 `demandOnly` 源，断言其声明字段全部返回（push worker 记 skip，读由 sink 驱动）。

### 既有 profiles
- core/materialization/migration/resources/cancel/placement 保持；`all` 顺序稳定。

## 3. CLI 与报告

- `driver-check --package <dir> --profile all|core|...|streaming`.
- 五级仍为 pass/fail/warn/skip；manual 级别保留枚举但不产生（兼容历史报告）。
- `driver-check inventory --data-dir <dir>`：列出 built-in manifests、已安装包（driver/version/abi/hash）、发现 v1 `.so`（拒绝清单），供迁移计划使用。
- 全套 `all` 对 demo-declarative 全部自动 profile 通过（skip 允许，fail/warn 需解释）。

## 4. 迁移计划文档

`docs/superpowers/plans/2026-10-23-e4e-migration-plan.md` 覆盖：

1. 存量盘点：simulated/modbus_rtu/snmp 内置；drivers/{bacnet,modbus,mqtt,mqtt-bridge,onvif,opcua,snmp}；外部已部署 v1 `.so`。
2. 内置驱动：E4a 已迁 v2/manifest；双层声明（describe）。
3. `drivers/*`：纳入 workspace 编译门禁、补 manifest/ABI v2、按 mqtt → modbus/bacnet/opcua/snmp → onvif → mqtt-bridge 顺序迁移。
4. 外部 `.so`：v1 硬切；用 v2 SDK 重建 → staging 验收 → 维护窗口原子替换 hub+驱动；回滚重装旧二进制。
5. 数据迁移：物声明重绑、模板能力投影 drift、maps 填充前的 `zone_source=hierarchy`、position_ingest 回填。
6. inventory 命令使用与验收 gate（demo 全套 + 迁移后的驱动逐包验收）。
7. 风险与回滚演练。

## 5. 退出标准

- demo-declarative 通过 core/materialization/migration/resources/cancel/placement/lifecycle/telemetry/tools/streaming 全部自动 profile（无 fail；skip 有理由）。
- `driver-check inventory` 能盘点包与 v1 产物。
- 迁移计划文档覆盖内置/`drivers/*`/外部 `.so`/数据/空间/回滚，并含 runbook。
- 全量 workspace/clippy/web 回归通过。
- 不实现 manual/`--interactive`（显式裁定）。

## 6. 非目标

- manual/物理硬件交互、`--interactive`。
- 迁移执行本身（另立阶段）。
- E3 World 2D 渲染、地球锚定。
- 第三方驱动签名/信任治理（E5）。
