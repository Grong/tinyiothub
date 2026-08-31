# 设备管理 (Device Management)

管理和操作 IoT 设备：搜索、查看、属性读写、命令下发、设备生命周期管理。

## 可用工具

| 工具 | 用途 | 风险 |
|---|---|---|
| `search_things` | 列出/搜索设备，支持按名称、类型、状态过滤 | 低 |
| `get_thing` | 获取设备完整信息：属性列表、标签、元数据 | 低 |
| `read_properties` | 读取设备的实时属性值 | 低 |
| `write_properties` | 修改设备属性值 | **高** |
| `send_command` | 向设备下发命令（如重启、切换模式） | **高** |
| `create_thing` | 在工作空间中注册新设备 | 中 |
| `delete_thing` | 删除设备（不可恢复） | **高** |

## 典型场景

### 查询设备
- "有哪些设备在线？" → `search_things` 列出所有设备
- "传感器 A 的温度是多少？" → `search_things` 找到设备 → `read_properties` 读取温度
- "设备 B 的详细信息" → `get_thing` 获取完整 Profile

### 控制设备
- "把空调调到 26 度" → `search_things` 确认设备 → 向用户确认 → `write_properties` 写入
- "重启网关设备" → `search_things` 确认设备 → 向用户确认 → `send_command`

### 设备生命周期
- "帮我加一个新传感器" → `create_thing`
- "删除这个不再使用的设备" → 向用户确认影响范围 → `delete_thing`

## 注意事项
- `write_properties`、`send_command`、`delete_thing` 必须先获得用户明确确认
- 属性值类型必须匹配（字符串、数字、布尔），写入前先 `read_properties` 了解类型
- 离线设备无法写入属性或下发命令，先检查设备状态
- `create_thing` 需要知道设备名称、Driver 类型和连接参数
