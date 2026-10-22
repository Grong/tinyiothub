# E4d 空间模型子 spec（maps / places / zone 权威）

- 日期：2026-10-22
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md`（完整 maps/place/zone、改写裁定 #3、maps 为 zone 权威）
- 范围：E4d。E4e（全套验收）不在本期
- 状态：设计两问已与作者确认（2026-10-22）

## 0. 关键裁定

| # | 主题 | 裁定 |
|---|---|---|
| 1 | 地图帧 | 工作区本地 2D 平面：`maps{name,bounds{x,y,w,h},anchor_map_id?,is_default}`；map 内坐标 x/y 米、level 整数、heading 可选；支持父图嵌套；**不做地球锚定/帧旋转** |
| 2 | 本期范围 | 数据模型 + CRUD API + 坐标/trust + point-in-polygon + zone 权威切换 + placement 物化 + `placement` 验收 profile；**E3 World 地图渲染另立**（现有 zone 名称投影保持可用） |
| 3 | zone 语义 | 显式 `placement.zoneRef` 优先；否则在 thing.map_id（缺省=workspace 默认图）的 zone 多边形中做 point-in-polygon；命中多个取面积最小；边界含入；不支持洞 |
| 4 | 权威切换 | maps 为 zone 权威（改写架构裁定 #3）；`workspaces.zone_source = 'hierarchy'|'maps'`，存量默认 hierarchy、新工作区默认 maps；placement 验收通过后再切；hierarchy 为迁移期回退 |
| 5 | trust | `fixed` 设备恒 trusted；runtime 按 age：≤30s trusted / ≤5min uncertain / >5min lost（workspace 可配）；驱动上报 trust 仅作上限，age 可降级 |
| 6 | 位置输入 | 驱动 source kind=`position`（保留字段 `position_x/position_y/position_level/position_heading/position_trust`）或 `PUT /v1/things/{id}/position` 人工校正；API 越界拒绝；驱动越界 → zone=None + warn |
| 7 | 管理鉴权 | maps/places/position 管理沿用 driver install 的 admin 闸门；读取走 workspace scope |

## 1. 组件与边界

```
crates/db/
  migrations/20261022000001_spatial.sql   maps / places / things 坐标列 / workspaces.zone_source
  src/map.rs                               Map/Place CRUD + position 列读写
crates/runtime/src/maps/
  model.rs                                 Map/Place/Point/Polygon/Trust
  zone.rs                                  point-in-polygon、最小面积、trust 派生
  service.rs                               ZoneService（读取 maps/places，缓存+失效）
crates/runtime/src/watch/live_state.rs     zone 摘要改为 ZoneService（flag=maps），hierarchy 回退
crates/runtime/src/ports.rs                MapStore / ZoneSource
crates/runtime/src/data_server.rs          position 保留字段 → things 坐标列
apps/cloud/src/domains/map/                /v1/maps、/v1/maps/{id}/places CRUD（admin）
apps/cloud/src/domains/thing/              PUT /v1/things/{id}/position
crates/runtime/src/acceptance/profiles/placement.rs
```

边界：

1. 几何/zone 纯函数在 runtime（无 db）；持久化经 `MapStore` port。
2. `thing.map_id` 为空 → workspace 默认图（`maps.is_default=1`）；没有默认图/没有 zone → zone=None。
3. maps 权威仅在 `zone_source='maps'` 时生效；hierarchy 回退保留 E1b 行为，迁移完成后删除。
4. E3 World 继续读 LiveState 摘要的 zone 字符串；地图/多边形渲染不在本期。
5. 多租户：maps/places/position 全部 workspace 隔离。

## 2. 契约

### 2.1 数据模型

```rust
pub struct MapBounds { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }

pub enum PlaceKind { Landmark, Zone }

pub enum Geometry {
    Point { x: f64, y: f64 },
    Polygon { points: Vec<(f64, f64)> }, // >=3
}

pub struct Map { id, workspace_id, name, bounds, anchor_map_id: Option<String>, is_default: bool }
pub struct Place { id, map_id, name, kind, geometry, heading: Option<f64> }

pub enum LocationTrust { Trusted, Uncertain, Lost }
```

DB：

- `maps(id, workspace_id, name, bounds_json, anchor_map_id, is_default, created_at, updated_at)`；`UNIQUE(workspace_id,name)`；每 workspace 至多一个 `is_default=1`（应用层校验）。
- `places(id, map_id, name, kind, geometry_json, heading, created_at, updated_at)`；`UNIQUE(map_id,name)`；zone 必须 polygon≥3。
- `things` 增列：`map_id, position_x, position_y, position_level, position_heading, position_trust, position_updated_at`（additive）。
- `workspaces` 增列 `zone_source TEXT NOT NULL DEFAULT 'hierarchy'`（新工作区创建时写 'maps'）。

### 2.2 zone 与 trust

- `resolve_zone(workspace, thing, now) -> Option<Place>`：
  1. `placement.zoneRef`（实例声明/物列）命中同 workspace place（kind=Zone）→ 返回。
  2. `thing.map_id` 或默认图；position 缺失 → None。
  3. 所有 Zone 多边形 point-in-polygon（边界含入）；取面积最小；无命中 → None。
- `derive_trust(placement_mode, position_updated_at, driver_trust, now)`：
  - fixed → Trusted；
  - runtime → age ≤30s Trusted / ≤5min Uncertain / >5min Lost；driver_trust 只能降级（取更差者）。
- `zone_source='hierarchy'` 或无 maps 数据 → 现有 parent_id 层级派生；`maps` → ZoneService 结果（None 即 None，不回退层级）。

### 2.3 placement 物化与位置输入

- 建物/绑定驱动时：manifest `placement.mode=fixed|declared` 且带 position/mapRef/zoneRef → 写坐标列；map/zone 名不存在 → 绑定失败/验收 fail（fail-closed）。
- `placement.mode=runtime` 且 `fromSource`：驱动 source kind=`position` 的读数保留字段写入坐标列：
  `position_x/position_y/position_level/position_heading/position_trust`；`position_updated_at=now`。
- `PUT /v1/things/{id}/position`：body `{x,y,level?,heading?,trust?}`；越界/非默认图 → 400；写列并更新 updated_at。
- 位置越界（驱动）：保留坐标但 zone=None + warn（不拒绝读数）。

### 2.4 API

- `GET/POST/PATCH/DELETE /v1/maps`；`GET/POST/PATCH/DELETE /v1/maps/{id}/places`（admin）。
- `GET /v1/things/{id}/position`；`PUT /v1/things/{id}/position`（admin）。
- `GET /v1/workspaces/{id}/spatial/zone?thing=...` 可选调试读面（admin）。

## 3. 验收（placement profile）

- map/place 引用存在；bounds 合法；zone polygon≥3；默认图唯一。
- point-in-polygon 边界、最小面积、多 zone 选择。
- trust：fixed、30s/5min 阈值、driver trust 降级。
- 位置越界 → zone None + warn；`zone_source` 切换后 hierarchy 回退不进 maps 模式。
- 报告五级；加入 `all` 与 driver-check。

## 4. 退出标准

- 创建 maps/places 后，设备坐标落区正确；多 zone 取最小；显式 zoneRef 优先。
- `zone_source=maps` 时 LiveState/read_device 的 zone 来自 maps；hierarchy 回退仍可用；切换可回滚。
- 驱动 position source 与 PUT position 都能更新坐标/trust；越界行为符合错误矩阵。
- placement 验收 profile 通过；E3 World 现有 zone 名称投影不回归。
- 多租户 map/place/position 隔离。

## 5. 非目标

- 地球锚定/帧旋转/坐标系换算（AGH 完整帧）。
- E3 World 2D 地图/多边形渲染与地图编辑 UI（另立）。
- 室内定位算法/指纹/蓝牙信标（只消费坐标）。
- 移动机器人运动安全（裁定 #15）。
