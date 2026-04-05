const translation = {
  // 页面信息
  pageTitle: 'Device Management',
  pageDescription: 'Manage and monitor all IoT devices in your system',
  
  // 设备基本信息
  deviceName: 'Device Name',
  deviceType: 'Device Type',
  deviceModel: 'Device Model',
  deviceStatus: 'Device Status',
  deviceDescription: 'Description',
  ipAddress: 'IP Address',
  port: 'Port',
  protocol: 'Protocol',
  driver: 'Driver',
  location: 'Location',
  manufacturer: 'Manufacturer',
  duplicate:'Copy',
  // 设备状态
  status: {
    all: 'All Devices',
    online: 'Online',
    offline: 'Offline',
    error: 'Error',
    maintenance: 'Maintenance',
    connecting: 'Connecting',
    disconnected: 'Disconnected',
  },
  
  // 操作按钮
  actions: {
    addDevice: 'Add Device',
    editDevice: 'Edit Device',
    deleteDevice: 'Delete Device',
    viewDetails: 'View Details',
    connectDevice: 'Connect',
    disconnectDevice: 'Disconnect',
    refreshStatus: 'Refresh Status',
    exportData: 'Export Data',
  },
  
  // 表单字段
  form: {
    name: 'Device Name',
    nameRequired: 'Device name is required',
    namePlaceholder: 'Enter device name',
    displayName: 'Display Name',
    displayNamePlaceholder: 'Enter display name',
    type: 'Device Type',
    typeRequired: 'Device type is required',
    typePlaceholder: 'Select device type',
    model: 'Device Model',
    modelPlaceholder: 'Enter device model',
    description: 'Description',
    descriptionPlaceholder: 'Enter device description',
    ipAddress: 'IP Address',
    ipAddressRequired: 'IP address is required',
    ipAddressPlaceholder: 'Enter IP address',
    port: 'Port',
    portPlaceholder: 'Enter port number',
    protocol: 'Protocol',
    protocolPlaceholder: 'Select protocol',
    driver: 'Driver',
    driverPlaceholder: 'Select driver',
    location: 'Location',
    locationPlaceholder: 'Enter device location',
    manufacturer: 'Manufacturer',
    manufacturerPlaceholder: 'Enter manufacturer',
  },
  
  // 设备属性
  properties: 'Properties',
  commands: 'Commands',
  alarms: 'Alarms',
  lastSeen: 'Last Seen',
  createdAt: 'Created At',
  updatedAt: 'Updated At',
  
  // 消息提示
  messages: {
    createSuccess: 'Device created successfully',
    createFailed: 'Failed to create device',
    updateSuccess: 'Device updated successfully',
    updateFailed: 'Failed to update device',
    deleteSuccess: 'Device deleted successfully',
    deleteFailed: 'Failed to delete device',
    deleteConfirm: 'Are you sure you want to delete device "{{name}}"?',
    deleteTip: 'This action cannot be undone and will remove all associated data.',
    connectSuccess: 'Device connected successfully',
    connectFailed: 'Failed to connect to device',
    disconnectSuccess: 'Device disconnected successfully',
    disconnectFailed: 'Failed to disconnect device',
    noDevices: 'No devices found',
    noDescription: 'No description available',
    noTimeInfo: 'No time information',
    loadingDevices: 'Loading devices...',
    refreshSuccess: 'Device status refreshed',
    refreshFailed: 'Failed to refresh device status',
  },
  
  // 设备相关
  unknownProduct: 'Unknown Product',
  deviceDeleted: 'Device deleted successfully',
  deviceDeleteFailed: 'Failed to delete device',
  editDevice: 'Edit Device',
  noDescription: 'No description available',
  noTimeInfo: 'No time information',
  deleteDeviceConfirmTitle: 'Delete Device',
  deleteDeviceConfirmContent: 'Are you sure you want to delete this device? This action cannot be undone.',
  
  // 创建设备相关
  createDevice: 'Create Device',
  createDeviceDescription: 'Add a new IoT device to your system',
  noDevices: 'No devices found',
  noDevicesDescription: 'Click the button above to create your first device',
  
  // 设备创建方式
  creation: {
    title: 'Create Device',
    recommended: 'Recommended',
    fromTemplate: 'Create from device template',
    manual: {
      title: 'Create Device',
      description: 'Manually configure device parameters',
      preview: {
        title: 'Manual Configuration',
        description: 'Fill in device information manually for custom setup',
      },
    },
    import: {
      title: 'Import JSON File',
      description: 'Import device configuration from JSON file',
      selectFile: 'Select Configuration File',
      dragDrop: 'Drag and drop your JSON file here',
      supportedFormats: 'Supports .json files only',
      browse: 'Browse Files',
      selectAnother: 'Select Another File',
      preview: 'Device Preview',
      fileValidated: 'File validated successfully',
      import: 'Import Device',
      preview: {
        title: 'Import Configuration',
        description: 'Import device settings from a JSON configuration file',
      },
      errors: {
        invalidFileType: 'Please select a valid JSON file',
        invalidJson: 'Invalid JSON format',
        missingName: 'Device name is required in the configuration',
      },
    },
  },
  
  // 时间格式化
  dateTimeFormat: 'MM/DD/YYYY HH:mm',
  editedAt: 'Updated at',
  
  // 筛选和显示
  showMyCreatedDevicesOnly: 'Show only devices I created',
  loading: 'Loading...',
  loadFailed: 'Failed to load',
  
  // 设备详情页面
  details: {
    overview: 'Overview',
    monitoring: 'Monitoring',
    configuration: 'Configuration',
    basicInfo: 'Basic Information',
    connectionInfo: 'Connection Information',
    deviceInfo: 'Device Information',
    systemInfo: 'System Information',
    performanceMetrics: 'Performance Metrics',
    recentEvents: 'Recent Events',
    deviceTags: 'Device Tags',
    deviceProperties: 'Device Properties',
    deviceCommands: 'Device Commands',
    deviceEvents: 'Device Events',
    statistics: 'Statistics',
    propertiesTab: 'Properties',
    commandsTab: 'Commands',
    eventsTab: 'Events',
  },

  // 设备详情页面内容
  overview: 'Overview',
  monitoring: 'Monitoring',
  configuration: 'Configuration',
  dataChart: 'Data Chart',
  recentEvents: 'Recent Events',
  
  // 设备信息
  productName: 'Product Name',
  deviceId: 'Device ID',
  neverSeen: 'Never Online',
  unknown: 'Unknown',
  
  // 即将推出的功能
  eventsComingSoon: 'Events feature coming soon',
  eventsDescription: 'Here you will be able to view device historical events and logs',
  monitoringComingSoon: 'Monitoring feature coming soon',
  monitoringDescription: 'Here you will be able to monitor device status and performance metrics in real-time',
  configurationComingSoon: 'Configuration feature coming soon',
  configurationDescription: 'Here you will be able to modify device configuration parameters and settings',
  chartComingSoon: 'Chart feature coming soon',
  chartDescription: 'Here you will be able to view visualized charts of device data',
  
  // 筛选和搜索
  filter: {
    all: 'All Devices',
    byStatus: 'Filter by Status',
    byType: 'Filter by Type',
    byLocation: 'Filter by Location',
    searchPlaceholder: 'Search devices...',
    clearFilters: 'Clear Filters',
  },
  
  // 设备类型
  types: {
    sensor: 'Sensor',
    camera: 'Camera',
    controller: 'Controller',
    gateway: 'Gateway',
    actuator: 'Actuator',
    robot: 'Robot',
    unknown: 'Unknown',
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