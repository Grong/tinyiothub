# 设备模板系统需求文档

## 介绍

设备模板系统旨在简化设备添加流程，通过预定义的JSON模板描述设备的基本信息、属性和命令结构，让用户能够快速选择合适的设备模板并填写必要信息来创建设备。

## 术语表

- **Device_Template**: 设备模板，包含设备基本信息、属性和命令的JSON结构定义
- **Template_Engine**: 模板引擎，负责解析和应用设备模板
- **Device_Wizard**: 设备创建向导，基于模板的设备添加界面
- **Template_Repository**: 模板仓库，存储和管理所有设备模板
- **Property_Template**: 属性模板，定义设备属性的结构
- **Command_Template**: 命令模板，定义设备命令的结构

## 需求

### 需求 1: 设备模板管理

**用户故事:** 作为系统管理员，我希望能够管理设备模板，以便为不同类型的设备提供标准化的创建模板。

#### 验收标准

1. WHEN 系统启动时，THE Template_Repository SHALL 加载内置的设备模板
2. WHEN 管理员访问模板管理页面时，THE System SHALL 显示所有可用的设备模板列表
3. WHEN 管理员查看模板详情时，THE System SHALL 显示模板的完整JSON结构和预览信息
4. WHEN 管理员创建新模板时，THE System SHALL 验证JSON格式并保存到模板仓库
5. WHEN 管理员修改现有模板时，THE System SHALL 保留模板版本历史并更新当前版本
6. WHEN 管理员删除模板时，THE System SHALL 检查是否有设备正在使用该模板并给出警告

### 需求 2: 设备模板结构定义

**用户故事:** 作为开发者，我希望设备模板有标准化的JSON结构，以便系统能够正确解析和应用模板。

#### 验收标准

1. THE Device_Template SHALL 包含设备基本信息字段（名称、类型、厂商、型号等）
2. THE Device_Template SHALL 包含属性模板数组，定义设备的所有属性
3. THE Device_Template SHALL 包含命令模板数组，定义设备的所有可用命令
4. THE Property_Template SHALL 包含属性名称、显示名称、数据类型、单位、范围等字段
5. THE Command_Template SHALL 包含命令名称、显示名称、参数定义等字段
6. THE Device_Template SHALL 支持模板元数据（版本、作者、描述、分类等）
7. WHEN 模板包含无效字段时，THE Template_Engine SHALL 返回详细的验证错误信息

### 需求 3: 基于模板的设备创建向导

**用户故事:** 作为用户，我希望通过选择设备模板来快速创建设备，而不需要手动配置所有属性和命令。

#### 验收标准

1. WHEN 用户点击"添加设备"时，THE Device_Wizard SHALL 显示设备模板选择界面
2. WHEN 用户浏览模板时，THE System SHALL 按分类显示模板并提供搜索功能
3. WHEN 用户选择模板时，THE Device_Wizard SHALL 显示模板预览和基本信息
4. WHEN 用户确认模板选择时，THE Device_Wizard SHALL 显示基于模板的设备配置表单
5. WHEN 用户填写设备信息时，THE System SHALL 根据模板预填充默认值
6. WHEN 用户提交设备创建时，THE System SHALL 根据模板自动创建设备属性和命令
7. WHEN 创建过程出错时，THE System SHALL 显示详细错误信息并允许用户修正

### 需求 4: 模板API接口

**用户故事:** 作为前端开发者，我需要API接口来获取和管理设备模板，以便在用户界面中展示和使用模板。

#### 验收标准

1. THE System SHALL 提供GET /api/device-templates接口获取模板列表
2. THE System SHALL 提供GET /api/device-templates/{id}接口获取特定模板详情
3. THE System SHALL 提供GET /api/device-templates/categories接口获取模板分类
4. THE System SHALL 支持按分类、厂商、设备类型筛选模板
5. THE System SHALL 提供POST /api/devices/from-template接口基于模板创建设备
6. WHEN API请求包含无效参数时，THE System SHALL 返回400错误和详细错误信息
7. WHEN 请求的模板不存在时，THE System SHALL 返回404错误

### 需求 5: 内置设备模板

**用户故事:** 作为用户，我希望系统提供常见设备类型的内置模板，以便快速开始使用系统。

#### 验收标准

1. THE System SHALL 内置Modbus RTU设备模板，包含常见的寄存器读写属性
2. THE System SHALL 内置MQTT设备模板，包含发布和订阅命令
3. THE System SHALL 内置SNMP设备模板，包含OID读取属性
4. THE System SHALL 内置ONVIF摄像头模板，包含视频流和PTZ控制命令
5. THE System SHALL 内置温湿度传感器模板，包含温度和湿度属性
6. THE System SHALL 内置智能开关模板，包含开关状态属性和控制命令
7. WHEN 系统首次启动时，THE System SHALL 自动加载所有内置模板

### 需求 6: 模板验证和错误处理

**用户故事:** 作为系统管理员，我希望系统能够验证模板的正确性，以确保基于模板创建的设备能够正常工作。

#### 验收标准

1. WHEN 加载模板时，THE Template_Engine SHALL 验证JSON格式的正确性
2. WHEN 验证模板时，THE System SHALL 检查必需字段是否存在
3. WHEN 验证属性模板时，THE System SHALL 检查数据类型和范围的有效性
4. WHEN 验证命令模板时，THE System SHALL 检查参数定义的完整性
5. WHEN 模板引用不存在的驱动时，THE System SHALL 返回验证错误
6. WHEN 模板包含重复的属性或命令名称时，THE System SHALL 返回验证错误
7. IF 模板验证失败，THEN THE System SHALL 记录详细错误日志并拒绝加载模板

### 需求 7: 模板国际化支持

**用户故事:** 作为国际用户，我希望设备模板支持多语言显示，以便更好地理解和使用模板。

#### 验收标准

1. THE Device_Template SHALL 支持多语言的显示名称和描述
2. THE Property_Template SHALL 支持多语言的属性显示名称和单位
3. THE Command_Template SHALL 支持多语言的命令显示名称和参数描述
4. WHEN 用户切换语言时，THE System SHALL 显示对应语言的模板信息
5. WHEN 模板缺少某种语言时，THE System SHALL 回退到默认语言（中文）
6. THE System SHALL 支持中文和英文两种语言
7. WHEN 添加新语言时，THE System SHALL 提供语言包扩展机制

### 需求 8: 模板搜索和筛选

**用户故事:** 作为用户，我希望能够快速找到需要的设备模板，以便高效地创建设备。

#### 验收标准

1. WHEN 用户在模板选择界面输入关键词时，THE System SHALL 实时搜索匹配的模板
2. WHEN 用户选择设备分类时，THE System SHALL 筛选显示该分类下的所有模板
3. WHEN 用户选择厂商筛选时，THE System SHALL 显示该厂商的所有设备模板
4. WHEN 用户选择协议类型时，THE System SHALL 显示支持该协议的所有模板
5. THE System SHALL 支持按模板名称、描述、标签进行模糊搜索
6. THE System SHALL 支持多条件组合筛选
7. WHEN 搜索无结果时，THE System SHALL 显示友好的无结果提示和建议
