const translation = {
  // 页面信息
  pageTitle: '设备管理',
  pageDescription: '管理和监控系统中的所有物联网设备',
  
  // 设备基本信息
  deviceName: '设备名称',
  deviceType: '设备类型',
  deviceModel: '设备型号',
  deviceStatus: '设备状态',
  deviceDescription: '设备描述',
  ipAddress: 'IP地址',
  port: '端口',
  protocol: '协议',
  driver: '驱动',
  location: '位置',
  manufacturer: '制造商',
  duplicate:'复制',
  // 设备状态
  status: {
    all: '全部设备',
    online: '在线',
    offline: '离线',
    error: '故障',
    maintenance: '维护中',
    connecting: '连接中',
    disconnected: '已断开',
  },
  
  // 操作按钮
  actions: {
    addDevice: '添加设备',
    editDevice: '编辑设备',
    deleteDevice: '删除设备',
    viewDetails: '查看详情',
    connectDevice: '连接',
    disconnectDevice: '断开连接',
    refreshStatus: '刷新状态',
    exportData: '导出数据',
  },
  
  // 表单字段
  form: {
    name: '设备名称',
    nameRequired: '设备名称为必填项',
    namePlaceholder: '请输入设备名称',
    displayName: '显示名称',
    displayNamePlaceholder: '请输入显示名称',
    type: '设备类型',
    typeRequired: '设备类型为必填项',
    typePlaceholder: '请选择设备类型',
    model: '设备型号',
    modelPlaceholder: '请输入设备型号',
    description: '设备描述',
    descriptionPlaceholder: '请输入设备描述',
    ipAddress: 'IP地址',
    ipAddressRequired: 'IP地址为必填项',
    ipAddressPlaceholder: '请输入IP地址',
    port: '端口',
    portPlaceholder: '请输入端口号',
    protocol: '协议',
    protocolPlaceholder: '请选择协议',
    driver: '驱动',
    driverPlaceholder: '请选择驱动',
    location: '位置',
    locationPlaceholder: '请输入设备位置',
    manufacturer: '制造商',
    manufacturerPlaceholder: '请输入制造商',
  },
  
  // 设备属性
  properties: '属性',
  commands: '命令',
  alarms: '告警',
  lastSeen: '最后在线',
  createdAt: '创建时间',
  updatedAt: '更新时间',
  
  // 消息提示
  messages: {
    createSuccess: '设备创建成功',
    createFailed: '设备创建失败',
    updateSuccess: '设备更新成功',
    updateFailed: '设备更新失败',
    deleteSuccess: '设备删除成功',
    deleteFailed: '设备删除失败',
    deleteConfirm: '确定要删除设备 "{{name}}" 吗？',
    deleteTip: '此操作无法撤销，将删除所有相关数据。',
    connectSuccess: '设备连接成功',
    connectFailed: '设备连接失败',
    disconnectSuccess: '设备断开连接成功',
    disconnectFailed: '设备断开连接失败',
    noDevices: '未找到设备',
    noDescription: '暂无描述',
    noTimeInfo: '暂无时间信息',
    loadingDevices: '正在加载设备...',
    refreshSuccess: '设备状态刷新成功',
    refreshFailed: '设备状态刷新失败',
  },
  
  // 设备相关
  unknownProduct: '未知产品',
  deviceDeleted: '设备删除成功',
  deviceDeleteFailed: '设备删除失败',
  editDevice: '编辑设备',
  noDescription: '暂无描述',
  noTimeInfo: '暂无时间信息',
  deleteDeviceConfirmTitle: '删除设备',
  deleteDeviceConfirmContent: '确定要删除此设备吗？此操作无法撤销。',
  
  // 创建设备相关
  createDevice: '创建设备',
  createDeviceDescription: '添加新的物联网设备到系统中',
  noDevices: '暂无设备',
  noDevicesDescription: '点击上方按钮创建您的第一个设备',
  
  // 设备创建方式
  creation: {
    title: '创建设备',
    recommended: '推荐',
    fromTemplate: '从设备模板创建',
    manual: {
      title: '创建设备',
      description: '手动配置设备参数',
      preview: {
        title: '手动配置',
        description: '手动填写设备信息进行自定义设置',
      },
    },
    import: {
      title: '导入 JSON 文件',
      description: '从 JSON 文件导入设备配置',
      selectFile: '选择配置文件',
      dragDrop: '拖拽您的 JSON 文件到此处',
      supportedFormats: '仅支持 .json 文件',
      browse: '浏览文件',
      selectAnother: '选择其他文件',
      preview: '设备预览',
      fileValidated: '文件验证成功',
      import: '导入设备',
      previewConfig: {
        title: '导入配置',
        description: '从 JSON 配置文件导入设备设置',
      },
      errors: {
        invalidFileType: '请选择有效的 JSON 文件',
        invalidJson: '无效的 JSON 格式',
        missingName: '配置中缺少设备名称',
      },
    },
  },
  
  // 时间格式化
  dateTimeFormat: 'YYYY/MM/DD HH:mm',
  editedAt: '更新于',
  
  // 筛选和显示
  showMyCreatedDevicesOnly: '仅显示我创建的设备',
  loading: '加载中...',
  loadFailed: '加载失败',
  
  // 设备详情页面
  details: {
    overview: '概览',
    events: '事件',
    monitoring: '监控',
    configuration: '配置',
    basicInfo: '基本信息',
    connectionInfo: '连接信息',
    deviceInfo: '设备信息',
    systemInfo: '系统信息',
    performanceMetrics: '性能指标',
    recentEvents: '最近事件',
    deviceTags: '设备标签',
    deviceProperties: '设备属性',
    deviceCommands: '设备命令',
  },

  // 设备详情页面内容
  overview: '概览',
  events: '事件',
  monitoring: '监控',
  configuration: '配置',
  dataChart: '数据图表',
  recentEvents: '最近事件',
  
  // 设备信息
  productName: '产品名称',
  deviceId: '设备ID',
  neverSeen: '从未上线',
  unknown: '未知',
  
  // 即将推出的功能
  eventsComingSoon: '事件功能即将推出',
  eventsDescription: '在这里您将能够查看设备的历史事件和日志',
  monitoringComingSoon: '监控功能即将推出',
  monitoringDescription: '在这里您将能够实时监控设备的运行状态和性能指标',
  configurationComingSoon: '配置功能即将推出',
  configurationDescription: '在这里您将能够修改设备的配置参数和设置',
  chartComingSoon: '图表功能即将推出',
  chartDescription: '在这里您将能够查看设备数据的可视化图表',
  
  // 筛选和搜索
  filter: {
    all: '全部设备',
    byStatus: '按状态筛选',
    byType: '按类型筛选',
    byLocation: '按位置筛选',
    searchPlaceholder: '搜索设备...',
    clearFilters: '清除筛选',
  },
  
  // 设备类型
  types: {
    sensor: '传感器',
    camera: '摄像头',
    controller: '控制器',
    gateway: '网关',
    actuator: '执行器',
    robot: '机器人',
    unknown: '未知设备',
  },
  
  // 协议类型
  protocols: {
    modbus: 'Modbus',
    onvif: 'ONVIF',
    snmp: 'SNMP',
    mqtt: 'MQTT',
    http: 'HTTP',
    tcp: 'TCP',
    udp: 'UDP',
  },
}

export default translation