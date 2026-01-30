const translation = {
  // 页面信息
  pageTitle: '设备模板',
  pageDescription: '选择合适的设备模板快速创建设备',
  
  // 模板基本信息
  templateName: '模板名称',
  templateDescription: '模板描述',
  templateCategory: '模板分类',
  templateVersion: '版本',
  templateAuthor: '作者',
  manufacturer: '制造商',
  deviceType: '设备类型',
  protocolType: '协议类型',
  driverName: '驱动名称',
  tags: '标签',
  
  // 模板分类
  categories: {
    all: '全部模板',
    title: '模板分类',
    sensors: '传感器',
    cameras: '摄像头',
    controllers: '控制器',
    robots: '机器人',
    actuators: '执行器',
    gateways: '网关',
  },
  
  // 设备类型
  deviceTypes: {
    sensor: '传感器',
    camera: '摄像头',
    controller: '控制器',
    robot: '机器人',
    actuator: '执行器',
    gateway: '网关',
    unknown: '未知',
  },
  
  // 协议类型
  protocolTypes: {
    modbus: 'Modbus RTU',
    mqtt: 'MQTT',
    onvif: 'ONVIF',
    snmp: 'SNMP',
    http: 'HTTP',
    tcp: 'TCP',
    udp: 'UDP',
  },
  
  // 操作按钮
  actions: {
    selectTemplate: '选择模板',
    previewTemplate: '预览模板',
    useTemplate: '使用此模板',
    backToList: '返回列表',
    backToTemplates: '返回模板',
    nextStep: '下一步',
    previousStep: '上一步',
    createDevice: '创建设备',
    cancel: '取消',
    search: '搜索',
    filter: '筛选',
    clearFilter: '清除筛选',
    createFromBlank: '从空白创建',
    selectAndContinue: '选择并继续',
    select: '选择',
  },
  
  // 搜索和筛选
  search: {
    placeholder: '搜索模板名称、描述或标签...',
    noResults: '未找到匹配的模板',
    noResultsDescription: '请尝试调整搜索条件或筛选器',
    filterByCategory: '按分类筛选',
    filterByManufacturer: '按制造商筛选',
    filterByProtocol: '按协议筛选',
    filterByType: '按设备类型筛选',
    foundResults: '找到 {{count}} 个结果',
    foundResult: '找到 {{count}} 个结果',
  },
  
  // 模板详情
  details: {
    basicInfo: '基本信息',
    properties: '属性模板',
    commands: '命令模板',
    preview: '预览',
    requirements: '必填字段',
    optional: '可选字段',
    defaultValue: '默认值',
    dataType: '数据类型',
    unit: '单位',
    range: '范围',
    readOnly: '只读',
    required: '必填',
    parameters: '参数',
  },
  
  // 设备创建向导
  wizard: {
    title: '基于模板创建设备',
    description: '从我们的设备模板集合中选择，快速开始',
    steps: {
      selectTemplate: '选择模板',
      configureDevice: '配置设备',
      preview: '预览确认',
      complete: '创建完成',
    },
    stepDescription: {
      selectTemplate: '从可用模板中选择最适合的设备模板',
      configureDevice: '填写设备基本信息和配置参数',
      preview: '预览设备配置并确认创建',
      complete: '设备创建成功',
    },
  },
  
  // 表单字段
  form: {
    deviceName: '设备名称',
    deviceNameRequired: '设备名称为必填项',
    deviceNamePlaceholder: '请输入设备名称',
    displayName: '显示名称',
    displayNamePlaceholder: '请输入显示名称',
    description: '设备描述',
    descriptionPlaceholder: '请输入设备描述',
    position: '设备位置',
    positionPlaceholder: '请输入设备位置',
    address: '设备地址',
    addressPlaceholder: '请输入设备地址（IP地址、串口等）',
    driverOptions: '驱动选项',
    driverOptionsPlaceholder: '请输入驱动配置选项（JSON格式）',
    propertyValues: '属性默认值',
    enabledCommands: '启用的命令',
    selectCommands: '选择要启用的命令',
    allCommands: '全部命令',
    requiredCommands: '必需命令',
    optionalCommands: '可选命令',
  },
  
  // 验证消息
  validation: {
    nameRequired: '设备名称不能为空',
    nameInvalid: '设备名称格式不正确',
    addressRequired: '设备地址不能为空',
    addressInvalid: '设备地址格式不正确',
    valueRequired: '此字段为必填项',
    valueInvalid: '字段值格式不正确',
    numberOutOfRange: '数值超出允许范围',
    jsonInvalid: 'JSON格式不正确',
  },
  
  // 消息提示
  messages: {
    loadingTemplates: '正在加载模板...',
    loadingTemplate: '正在加载模板详情...',
    loadingCategories: '正在加载分类...',
    templateNotFound: '模板不存在',
    templateLoadFailed: '模板加载失败',
    validationFailed: '输入验证失败',
    previewFailed: '预览生成失败',
    createSuccess: '设备创建成功',
    createFailed: '设备创建失败',
    noTemplates: '暂无可用模板',
    noTemplatesDescription: '请联系管理员添加设备模板',
    selectTemplateFirst: '请先选择一个模板',
    fillRequiredFields: '请填写所有必填字段',
    confirmCreate: '确认创建设备？',
    confirmCreateDescription: '将基于选择的模板创建新设备',
  },
  
  // 模板信息展示
  info: {
    builtinTemplate: '内置模板',
    customTemplate: '自定义模板',
    templateCount: '{{count}} 个模板',
    propertyCount: '{{count}} 个属性',
    commandCount: '{{count}} 个命令',
    lastUpdated: '最后更新',
    createdAt: '创建时间',
    version: '版本 {{version}}',
    author: '作者：{{author}}',
    noAuthor: '系统内置',
    noDescription: '暂无描述',
    noProperties: '无属性定义',
    noCommands: '无命令定义',
  },

  // 模板市场
  marketplace: {
    title: '设备模板市场',
    description: '发现并选择各种设备模板，包括',
    and_more: '等更多类型',
  },
  
  // 空状态
  empty: {
    title: '未找到模板',
    description: '没有模板符合您当前的搜索条件。请尝试调整搜索或筛选条件。',
  },
  
  // 标签
  labels: {
    template: '模板',
    category: '分类',
    type: '类型',
    protocol: '协议',
    manufacturer: '制造商',
    version: '版本',
    author: '作者',
    tags: '标签',
  },
  
  // 错误信息
  errors: {
    networkError: '网络连接失败',
    serverError: '服务器错误',
    templateNotFound: '模板不存在',
    validationError: '输入验证失败',
    createError: '设备创建失败',
    unknownError: '未知错误',
  },
  
  // 帮助信息
  help: {
    templateSelection: '选择最适合您设备的模板，模板包含了设备的基本配置和功能定义',
    deviceConfiguration: '根据模板要求填写设备信息，必填字段不能为空',
    propertyConfiguration: '配置设备属性的默认值，这些值将在设备创建后使用',
    commandSelection: '选择要为设备启用的命令，必需命令将自动启用',
    addressFormat: '设备地址格式取决于协议类型，如 Modbus 使用 IP:端口，串口使用 COM1 等',
    driverOptions: '驱动选项为 JSON 格式，包含协议特定的配置参数',
  },
}

export default translation