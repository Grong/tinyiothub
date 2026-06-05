# Scene3D A2UI 组件与 Workspace 资源库设计

> 为 TinyIoTHub AI Agent 提供 3D 建筑/空间场景可视化能力，以及支撑 AI 语义检索的 Workspace 多媒体资源管理。

## 1. 概述

### 1.1 目标

- 在 A2UI 组件体系中增加 `Scene3D` 类型，支持楼宇级 3D 场景展示
- 建立轻量的 Workspace Knowledge Base，让 AI Agent 能通过语义搜索定位场景资源
- 实现"用户自然语言 → AI 检索资源 → 获取实时数据 → A2UI 3D 渲染"的完整闭环

### 1.2 非目标

- 不做形式化 Ontology Schema（如 Brick Schema 的完整概念层）
- 不做独立向量数据库，SQLite + 标签 + 文本匹配即可
- 不做第一人称漫游（WASD 行走）
- 不做复杂 BIM 解析（如 IFC 格式支持）

### 1.3 成功标准

- Agent 能响应"3楼车间的温度传感器状态"，自动加载对应 3D 场景并展示设备状态
- 单场景支持 500+ 设备标记，帧率保持 30fps+
- 资源库支持 1000+ 资源/Workspace 的检索（亚秒级）

---

## 2. 背景与动机

当前 TinyIoTHub 的 A2UI 体系已支持丰富的 2D 组件（DeviceCard、DeviceTable、DataChart 等），但缺少空间维度的可视化能力。在工业物联网场景中：

- 用户需要直观看到设备在建筑中的**物理位置**
- 设备告警时需要在 3D 场景中**快速定位**
- 不同楼层/区域的设备需要**剖切查看**

同时，AI Agent 要渲染 3D 场景，必须先知道"这个 Workspace 有哪些 3D 场景、场景中有哪些设备"。这需要一个新的**Workspace 资源管理层**来弥合自然语言与结构化资源之间的鸿沟。

---

## 3. 架构总览

```
用户提问: "3楼车间的温度传感器状态如何？"
        ↓
┌──────────────────────────────────────────────────────────┐
│  Layer 3: AI Agent (zeroclaw)                             │
│  • LLM 推理 + Tool 调用                                    │
│  • search_workspace_resources Tool                        │
└──────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────┐
│  Layer 2: Workspace Knowledge Base (TinyIoTHub)           │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Resource Repository (SQLite)                       │  │
│  │  • workspace_resources 表                            │  │
│  │  • 文件存储: data/agents/{ws_id}/resources/          │  │
│  └────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Semantic Search (轻量)                             │  │
│  │  • 标签精确匹配 + 名称/描述 LIKE                      │  │
│  │  • 未来可扩展为 embedding 向量搜索                    │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
                            ↓ 返回 SceneResource + 设备列表
┌──────────────────────────────────────────────────────────┐
│  Layer 1: A2UI Rendering (Web Frontend)                   │
│  • Scene3D: Three.js + HTML Overlay                       │
│  • DeviceCard / DeviceTable: 设备详情                     │
│  • 其他 A2UI 组件                                          │
└──────────────────────────────────────────────────────────┘
```

### 3.1 核心设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 3D 引擎 | Three.js | Lit 生态最自然的集成方案，社区成熟 |
| 设备标记渲染 | HTML Overlay (DOM) | 标签清晰、点击简单、与 A2UI 组件联动自然 |
| 建筑模型 | GLB/GLTF 加载 | 标准格式，Blender/CAD 工具导出方便 |
| 资源搜索 | SQLite + 标签 + LIKE | 轻量、与现有技术栈统一、当前规模够用 |
| 设备位置存储 | SceneResource.metadata JSON | 与设备模型解耦，场景可独立编辑 |

---

## 4. Workspace Knowledge Base 设计

### 4.1 数据模型

新增 `workspace_resources` 表，与现有 `Workspace` 类型风格一致（`snake_case`、`Option<String>` 存 JSON）：

```rust
// cloud/src/modules/workspace/types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceResource {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: String,       // "scene", "device_model", "image", "document"
    pub name: String,
    pub description: Option<String>,
    pub file_path: String,           // 相对路径: scenes/factory.glb
    pub tags: Vec<String>,
    pub metadata: Option<String>,    // JSON 字符串
    pub created_at: String,
    pub updated_at: String,
}
```

#### resource_type 枚举值

| 类型 | 说明 | metadata 示例 |
|------|------|---------------|
| `scene` | 3D 场景（GLB/GLTF）| `{floors:[{id,name,level,yOffset,outline}], defaultCamera, deviceInstances:[{instanceId,deviceId,position,floorId}]}` |
| `device_model` | 设备 3D 模型 | `{category, manufacturer, thumbnail, defaultDimensions}` |
| `image` | 图片资产 | `{width, height, format}` |
| `document` | 文档/说明 | `{mime_type, page_count}` |

#### SQL Schema

```sql
CREATE TABLE workspace_resources (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    file_path TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',        -- JSON 数组字符串
    metadata TEXT,                          -- JSON 对象字符串
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_resources_workspace ON workspace_resources(workspace_id);
CREATE INDEX idx_resources_type ON workspace_resources(resource_type);
```

### 4.2 文件存储布局

与现有 workspace 文件系统保持统一：

```
data/agents/{workspace_id}/
├── memory/          # 已有
├── state/           # 已有
├── sessions/        # 已有
├── skills/          # 已有
├── cron/            # 已有
└── resources/       # 新增
    ├── scenes/
    │   └── factory_floor3.glb
    ├── device_models/
    │   └── temp_sensor.glb
    └── images/
        └── floor_plan.png
```

### 4.3 API 设计

```rust
// Handler 路由
GET    /api/workspaces/{id}/resources         // 列表 + 分页
POST   /api/workspaces/{id}/resources         // 创建（支持 multipart 上传文件）
GET    /api/workspaces/{id}/resources/{rid}   // 详情
DELETE /api/workspaces/{id}/resources/{rid}   // 删除
GET    /api/workspaces/{id}/resources/search?q=...&type=...&limit=10  // 语义搜索
```

### 4.4 搜索实现（Phase 1）

```rust
pub async fn search_resources(
    &self,
    workspace_id: &str,
    query: &str,
    resource_type: Option<&str>,
    limit: i64,
) -> Result<Vec<ResourceSearchResult>> {
    let keywords: Vec<String> = query
        .split_whitespace()
        .map(|s| format!("%{}%", s))
        .collect();

    // SQL: 匹配 tags OR name LIKE OR description LIKE
    // 使用简单相关性评分排序
    // 注：多关键词应循环 OR 拼接，示例简化只展示单关键词
    sqlx::query_as::<_, ResourceSearchResult>(r#"
        SELECT *, (
            (CASE WHEN name LIKE ?1 THEN 3 ELSE 0 END) +
            (CASE WHEN description LIKE ?1 THEN 2 ELSE 0 END) +
            (CASE WHEN EXISTS (
                SELECT 1 FROM json_each(tags) WHERE value LIKE ?1
            ) THEN 2 ELSE 0 END)
        ) as relevance
        FROM workspace_resources
        WHERE workspace_id = ?2
          AND (?3 IS NULL OR resource_type = ?3)
          AND (name LIKE ?1 OR description LIKE ?1 OR EXISTS (
              SELECT 1 FROM json_each(tags) WHERE value LIKE ?1
          ))
        ORDER BY relevance DESC
        LIMIT ?4
    "#)
    .bind(&keywords[0])
    .bind(workspace_id)
    .bind(resource_type)
    .bind(limit)
    .fetch_all(&self.pool)
    .await
}
```

**注意**：上述 SQL 只取了 `keywords[0]`，实际实现应循环所有关键词取 UNION。Phase 2 可引入 `fastembed-rs` 做本地 embedding，增加 `embedding` BLOB 字段做余弦相似度排序。

### 4.5 Agent Tool

新增 `search_workspace_resources` Tool，注册到 zeroclaw Agent：

```typescript
// Tool Schema
{
  name: "search_workspace_resources",
  description: "Search workspace multimedia resources (3D scenes, images, documents) by natural language query",
  parameters: {
    query: "Natural language search query, e.g. '3楼车间温度传感器'",
    resource_type: "Optional filter: 'scene', 'device_model', 'image', 'document'",
    limit: "Maximum results to return (default 10)"
  }
}

// 返回示例
{
  resources: [
    {
      id: "sc-factory-f3",
      resource_type: "scene",
      name: "3楼装配车间",
      description: "装配车间完整3D场景，包含12台温度传感器",
      file_path: "scenes/factory_floor3.glb",
      tags: ["3楼", "装配车间", "温度传感器", "生产线A"],
      metadata: { floors: [...], deviceInstances: [...] },
      relevance: 0.95
    }
  ]
}
```

---

## 5. Scene3D A2UI 组件设计

### 5.1 架构分层

```
┌─────────────────────────────────────────┐
│  HTML Overlay Layer (设备标记)            │
│  • DeviceMarker (div)                   │
│  • FloorLabel (div)                     │
│  • Tooltip/Popover (复用 A2UI Modal)     │
│  ← DOM, CSS 动画, 点击事件               │
├─────────────────────────────────────────┤
│  Three.js Canvas Layer (建筑结构)         │
│  • BuildingModel (GLB/Group)            │
│  • FloorMesh[]                          │
│  • AmbientLight + DirectionalLight      │
│  ← WebGL, OrbitControls                 │
├─────────────────────────────────────────┤
│  MiniMap Layer (Canvas 2D)              │
│  • 俯视图 + 相机位置指示器                │
│  • 设备位置标记                           │
└─────────────────────────────────────────┘
```

### 5.2 DataModel Schema

```typescript
interface Scene3DDataModel {
  // 场景引用
  resourceId: string;           // 引用 WorkspaceResource.id

  // 可选覆盖
  activeFloorId?: string;       // 当前激活楼层（剖切用）
  selectedDeviceId?: string;    // 当前选中设备

  // 设备过滤
  deviceFilter?: {
    floorId?: string;
    status?: ("online" | "offline" | "warning" | "error")[];
    deviceType?: string[];
  };

  // 交互配置
  interactions?: {
    enableOrbit?: boolean;       // 轨道控制（默认 true）
    enableFloorCut?: boolean;    // 楼层剖切（默认 true）
    showMiniMap?: boolean;       // 小地图（默认 true）
    deviceLabelMode?: "always" | "hover" | "never"; // 标签显示模式
  };
}
```

### 5.3 渲染流程

```
A2UI updateComponents(Scene3D)
        ↓
1. 初始化 Three.js
   • Scene, PerspectiveCamera, WebGLRenderer
   • OrbitControls (enableDamping=true)
   • 设置 renderer.domElement 尺寸
        ↓
2. 加载场景模型
   • 从 resource.file_path 读取 GLB
   • GLTFLoader → 解析为 THREE.Group
   • 自动计算场景包围盒，设置相机初始位置
        ↓
3. 解析场景元数据
   • 读取 metadata.floors → 生成楼层导航按钮
   • 读取 metadata.deviceInstances → 准备标记数据
        ↓
4. 加载设备实时数据
   • 调用 /api/devices?ids=[...] 获取状态
   • 与 deviceInstances 合并（deviceId → status）
        ↓
5. 渲染设备标记（HTML Overlay）
   • 世界坐标 → 屏幕坐标投影 (vector.project(camera))
   • 生成 <div class="a2ui-scene3d__marker">
   • CSS 设置状态颜色、脉冲动画
        ↓
6. 渲染小地图
   • Canvas 2D 绘制 floors[].outline
   • 标记设备位置
   • 绘制相机视角扇形指示器
        ↓
7. 启动动画循环
   • requestAnimationFrame 更新标记投影位置
   • OrbitControls 更新相机
   • 小地图相机指示器同步
```

### 5.4 设备标记投影（核心算法）

```typescript
function projectToScreen(
  worldPos: THREE.Vector3,
  camera: THREE.Camera,
  canvasWidth: number,
  canvasHeight: number,
): { x: number; y: number } {
  const vec = worldPos.clone().project(camera);
  return {
    x: (vec.x * 0.5 + 0.5) * canvasWidth,
    y: (-vec.y * 0.5 + 0.5) * canvasHeight,
  };
}

// 每帧更新
updateMarkers() {
  for (const m of this.deviceMarkers) {
    const { x, y } = projectToScreen(m.worldPos, this.camera, this.width, this.height);
    const isBehind = m.worldPos.clone().project(this.camera).z > 1;
    m.element.style.transform = `translate(${x}px, ${y}px)`;
    m.element.style.display = isBehind ? 'none' : 'block';
  }
}
```

### 5.5 交互设计

| 交互 | 实现 | 行为 |
|------|------|------|
| 旋转/缩放/平移 | OrbitControls | 左键旋转、滚轮缩放、右键平移 |
| 点击设备 | DOM click on marker | 触发 `onAction("selectDevice", {deviceId})` |
| 楼层切换 | 楼层按钮 | 更新 `activeFloorId`，ClippingPlanes 剖切 |
| 设备悬停 | CSS :hover | 显示设备名称 tooltip |
| 小地图点击 | Canvas click | 相机 flyTo 目标位置 |

### 5.6 楼层剖切

```typescript
updateFloorCut() {
  const floor = this.floors.find(f => f.id === this.activeFloorId);
  if (floor) {
    const floorHeight = floor.floorHeight || 3.5;
    this.renderer.clippingPlanes = [
      new THREE.Plane(new THREE.Vector3(0, -1, 0), floor.yOffset + floorHeight),
      new THREE.Plane(new THREE.Vector3(0, 1, 0), -floor.yOffset),
    ];
  } else {
    this.renderer.clippingPlanes = [];
  }
  // 同步过滤设备标记
  for (const m of this.deviceMarkers) {
    m.element.style.display = 
      !this.activeFloorId || m.floorId === this.activeFloorId ? 'block' : 'none';
  }
}
```

### 5.7 CSS 样式接口

```css
/* 容器 */
.a2ui-scene3d { position: relative; width: 100%; height: 400px; border-radius: 8px; overflow: hidden; }
.a2ui-scene3d__canvas { width: 100%; height: 100%; display: block; }

/* 设备标记 */
.a2ui-scene3d__marker {
  position: absolute;
  transform: translate(-50%, -100%);
  pointer-events: auto;
  cursor: pointer;
  transition: transform 0.1s;
}
.a2ui-scene3d__marker--online { --marker-color: #00d4aa; }
.a2ui-scene3d__marker--warning { --marker-color: #f59e0b; }
.a2ui-scene3d__marker--error { --marker-color: #ef4444; }
.a2ui-scene3d__marker--offline { --marker-color: #6b7280; }
.a2ui-scene3d__marker--pulse { animation: marker-pulse 2s infinite; }

@keyframes marker-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

/* 楼层导航 */
.a2ui-scene3d__floorbar {
  position: absolute; top: 12px; left: 12px;
  display: flex; flex-direction: column; gap: 4px;
}
.a2ui-scene3d__floor-btn {
  padding: 4px 12px; border-radius: 4px;
  background: rgba(0,0,0,0.5); color: #fff;
  border: none; cursor: pointer; font-size: 12px;
}
.a2ui-scene3d__floor-btn--active { background: rgba(0,212,170,0.8); }

/* 小地图 */
.a2ui-scene3d__minimap {
  position: absolute; bottom: 12px; right: 12px;
  width: 120px; height: 120px; border-radius: 4px;
  background: rgba(0,0,0,0.3);
}
```

---

## 6. Agent 数据闭环（完整示例）

```
用户: "3楼车间的温度传感器状态如何？"
  ↓
Agent (LLM)
  ↓
[Tool] search_workspace_resources
  query: "3楼车间温度传感器"
  → [{ id: "sc-factory-f3", type: "scene", name: "3楼装配车间", relevance: 0.95 }]
  ↓
Agent 读取 SceneResource.metadata
  → floors: [{id:"f3", name:"3楼装配车间", level:3, yOffset:9.0}]
  → deviceInstances: [
      {instanceId:"d1", deviceId:"sensor_001", position:[10,10.5,20], floorId:"f3"},
      ...
    ]
  ↓
[Tool] list_devices
  filter: { deviceIds: ["sensor_001", "sensor_002", ...] }
  → [{deviceId:"sensor_001", status:"online", temperature: 24.5, ...}, ...]
  ↓
Agent 决策: "Scene3D 展示场景 + DeviceTable 展示列表"
  ↓
A2UI 消息序列:

{"createSurface": {"id": "building-view", "surfaceKind": "inline"}}

{"updateComponents": {"surfaceId": "building-view", "components": [{
  "id": "scene-3d",
  "componentKind": "Scene3D",
  "dataModel": {
    "resourceId": "sc-factory-f3",
    "activeFloorId": "f3",
    "deviceFilter": {"floorId": "f3", "deviceType": ["temperature_sensor"]},
    "interactions": {"showMiniMap": true, "deviceLabelMode": "hover"}
  }
}]}}

{"updateComponents": {"surfaceId": "building-view", "components": [{
  "id": "device-table",
  "componentKind": "DeviceTable",
  "dataModel": {"devices": [...]}
}]}}
```

---

## 7. 文件组织

### 7.1 后端（Rust）

| 文件 | 内容 |
|------|------|
| `cloud/src/modules/workspace/types.rs` | 新增 `WorkspaceResource` 类型 |
| `cloud/src/modules/workspace/repo.rs` | 新增资源 CRUD + 搜索方法 |
| `cloud/src/modules/workspace/handler.rs` | 新增 REST API endpoints |
| `cloud/src/modules/workspace/service.rs` | 新增业务逻辑（文件上传/删除） |
| `cloud/src/modules/agent/tools/service.rs` | 注册 `search_workspace_resources` Tool |
| `cloud/migrations/00XX_workspace_resources.sql` | 数据库迁移 |

### 7.2 前端（TypeScript/Lit）

| 文件 | 内容 |
|------|------|
| `web/src/ui/chat/a2ui/catalog/scene-3d.ts` | Scene3D 组件实现 |
| `web/src/ui/chat/a2ui/catalog/index.ts` | 注册 `Scene3D: renderScene3D` |
| `web/src/styles/components/a2ui.css` | Scene3D 样式 |

### 7.3 外部依赖

- **前端新增**：`three` (Three.js), `@types/three`
- **后端新增**：无（复用现有 SQLite + axum）

---

## 8. 风险与备选方案

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Three.js 包体积大 (~500KB) | 前端加载慢 | 按需导入（只引入 core + OrbitControls + GLTFLoader），lazy load |
| GLB 模型文件大 | 网络传输慢 | 模型压缩（Draco）、CDN 分发、loading 状态提示 |
| 500+ 设备标记性能 | 帧率下降 | 视锥体裁剪（只渲染可见标记）、距离 LOD（远距离标记缩小/简化） |
| HTML Overlay 遮挡关系 | 标记穿过墙体显示 | 简易方案：相机后方隐藏；进阶：Raycaster 检测遮挡 |
| 多楼层 GLB 模型获取楼层边界困难 | 小地图/剖切不准 | 要求模型制作时按楼层分组命名，或 fallback 到 metadata.outline |

### 备选方案

如果 Three.js + HTML Overlay 方案在实践中遇到问题：

- **标记改 Sprite**：用 THREE.Sprite 渲染设备标记，文字用 Canvas 纹理生成。解决遮挡和清晰度问题，但增加交互复杂度。
- **模型改点云**：超大场景（1000+ 设备）时，建筑结构用简化几何体，设备标记用 Points 渲染。

---

## 9. 实现阶段

### Phase 1: Workspace 资源库（1-2 天）

1. 数据库迁移：`workspace_resources` 表
2. Repo 层：CRUD + 搜索
3. Handler 层：REST API（含 multipart 文件上传）
4. Service 层：文件存储到 `data/agents/{ws_id}/resources/`

### Phase 2: Agent Tool（0.5 天）

1. 实现 `search_workspace_resources` Tool
2. 注册到 zeroclaw Agent Tool 列表
3. 更新 TOOLS.md 文档

### Phase 3: Scene3D 组件核心（2-3 天）

1. Three.js 初始化 + OrbitControls
2. GLB 加载器 + 场景渲染
3. 设备标记 HTML Overlay + 投影
4. 楼层导航 + ClippingPlanes 剖切
5. 小地图 Canvas 2D

### Phase 4: 集成与优化（1-2 天）

1. 注册到 A2UI catalog
2. CSS 样式 + 主题适配
3. 性能测试（500 设备标记帧率）
4. Agent 端到端测试

---

## 10. CEO Review 决策记录

> 2026-05-28 `/plan-ceo-review` HOLD SCOPE 模式

### 审查中确认的关键决策

| # | 主题 | 决策 | 理由 |
|---|------|------|------|
| 1 | 文件上传安全 | 添加上传校验（大小限制 50MB、MIME 白名单、目录遍历防护） | CEO Review Section 3 关键发现 |
| 2 | GLB 加载错误处理 | 完整错误边界（错误占位符 + 重试按钮 + WebGL 不支持降级） | CEO Review Section 2 关键发现 |
| 3 | 500+ 设备标记性能 | 视锥体裁剪 + 距离 LOD | CEO Review Section 1/7 关键发现 |
| 4 | Agent 多搜索结果 | 自动取 relevance 最高场景 | 简化交互，错误时 Agent 可道歉并重新搜索 |
| 5 | 前端测试策略 | Playwright E2E + 手动视觉验证 | Three.js WebGL 组件无法 unit test |

### 审查中发现的其他问题（非阻塞）

- **Section 4:** 场景加载中用户离开需取消加载逻辑
- **Section 6:** 需要制定 Playwright E2E 测试基础设施计划
- **Section 8:** 缺少 Scene3D 加载时间指标和 GLB 加载失败日志
- **Section 11:** GLB 加载 spinner、空设备提示、移动端高度适配

## 11. 相关参考

- [用友 BIP 本体智能体发布](http://stock.finance.sina.com.cn/stock/go.php/vReport_Show/kind/search/rptid/821928969835/index.phtml)
- [Brick Schema — 智能建筑本体标准](https://brickschema.org/)
- [Azure Digital Twins DTDL](https://learn.microsoft.com/en-us/azure/digital-twins/concepts-models)
- [Three.js Documentation](https://threejs.org/docs/)
