const translation = {
  // Page information
  pageTitle: 'Device Templates',
  pageDescription: 'Select appropriate device templates to quickly create devices',
  
  // Template basic information
  templateName: 'Template Name',
  templateDescription: 'Template Description',
  templateCategory: 'Template Category',
  templateVersion: 'Version',
  templateAuthor: 'Author',
  manufacturer: 'Manufacturer',
  deviceType: 'Device Type',
  protocolType: 'Protocol Type',
  driverName: 'Driver Name',
  tags: 'Tags',
  
  // Template categories
  categories: {
    all: 'All Templates',
    title: 'Template Categories',
    sensors: 'Sensors',
    cameras: 'Cameras',
    controllers: 'Controllers',
    robots: 'Robots',
    actuators: 'Actuators',
    gateways: 'Gateways',
  },
  
  // Device types
  deviceTypes: {
    sensor: 'Sensor',
    camera: 'Camera',
    controller: 'Controller',
    robot: 'Robot',
    actuator: 'Actuator',
    gateway: 'Gateway',
    unknown: 'Unknown',
  },
  
  // Protocol types
  protocolTypes: {
    modbus: 'Modbus RTU',
    mqtt: 'MQTT',
    onvif: 'ONVIF',
    snmp: 'SNMP',
    http: 'HTTP',
    tcp: 'TCP',
    udp: 'UDP',
  },
  
  // Action buttons
  actions: {
    selectTemplate: 'Select Template',
    previewTemplate: 'Preview Template',
    useTemplate: 'Use This Template',
    backToList: 'Back to List',
    backToTemplates: 'Back to Templates',
    nextStep: 'Next Step',
    previousStep: 'Previous Step',
    createDevice: 'Create Device',
    cancel: 'Cancel',
    search: 'Search',
    filter: 'Filter',
    clearFilter: 'Clear Filter',
    createFromBlank: 'Create from Blank',
    selectAndContinue: 'Select & Continue',
    select: 'Select',
  },
  
  // Search and filter
  search: {
    placeholder: 'Search template name, description or tags...',
    noResults: 'No matching templates found',
    noResultsDescription: 'Try adjusting your search criteria or filters',
    filterByCategory: 'Filter by Category',
    filterByManufacturer: 'Filter by Manufacturer',
    filterByProtocol: 'Filter by Protocol',
    filterByType: 'Filter by Device Type',
    foundResults: 'Found {{count}} results',
    foundResult: 'Found {{count}} result',
  },
  
  // Template details
  details: {
    basicInfo: 'Basic Information',
    properties: 'Property Templates',
    commands: 'Command Templates',
    preview: 'Preview',
    requirements: 'Required Fields',
    optional: 'Optional Fields',
    defaultValue: 'Default Value',
    dataType: 'Data Type',
    unit: 'Unit',
    range: 'Range',
    readOnly: 'Read Only',
    required: 'Required',
    parameters: 'Parameters',
  },
  
  // Device creation wizard
  wizard: {
    title: 'Create Device from Template',
    description: 'Choose from our collection of device templates to get started quickly',
    steps: {
      selectTemplate: 'Select Template',
      configureDevice: 'Configure Device',
      preview: 'Preview & Confirm',
      complete: 'Creation Complete',
    },
    stepDescription: {
      selectTemplate: 'Choose the most suitable device template from available options',
      configureDevice: 'Fill in device basic information and configuration parameters',
      preview: 'Preview device configuration and confirm creation',
      complete: 'Device created successfully',
    },
  },
  
  // Form fields
  form: {
    deviceName: 'Device Name',
    deviceNameRequired: 'Device name is required',
    deviceNamePlaceholder: 'Enter device name',
    displayName: 'Display Name',
    displayNamePlaceholder: 'Enter display name',
    description: 'Device Description',
    descriptionPlaceholder: 'Enter device description',
    position: 'Device Position',
    positionPlaceholder: 'Enter device position',
    address: 'Device Address',
    addressPlaceholder: 'Enter device address (IP address, serial port, etc.)',
    driverOptions: 'Driver Options',
    driverOptionsPlaceholder: 'Enter driver configuration options (JSON format)',
    propertyValues: 'Property Default Values',
    enabledCommands: 'Enabled Commands',
    selectCommands: 'Select commands to enable',
    allCommands: 'All Commands',
    requiredCommands: 'Required Commands',
    optionalCommands: 'Optional Commands',
  },
  
  // Validation messages
  validation: {
    nameRequired: 'Device name cannot be empty',
    nameInvalid: 'Device name format is incorrect',
    addressRequired: 'Device address cannot be empty',
    addressInvalid: 'Device address format is incorrect',
    valueRequired: 'This field is required',
    valueInvalid: 'Field value format is incorrect',
    numberOutOfRange: 'Number is out of allowed range',
    jsonInvalid: 'JSON format is incorrect',
  },
  
  // Message prompts
  messages: {
    loadingTemplates: 'Loading templates...',
    loadingTemplate: 'Loading template details...',
    loadingCategories: 'Loading categories...',
    templateNotFound: 'Template not found',
    templateLoadFailed: 'Failed to load template',
    validationFailed: 'Input validation failed',
    previewFailed: 'Failed to generate preview',
    createSuccess: 'Device created successfully',
    createFailed: 'Failed to create device',
    noTemplates: 'No templates available',
    noTemplatesDescription: 'Please contact administrator to add device templates',
    selectTemplateFirst: 'Please select a template first',
    fillRequiredFields: 'Please fill in all required fields',
    confirmCreate: 'Confirm device creation?',
    confirmCreateDescription: 'A new device will be created based on the selected template',
  },
  
  // Template information display
  info: {
    builtinTemplate: 'Built-in Template',
    customTemplate: 'Custom Template',
    templateCount: '{{count}} templates',
    propertyCount: '{{count}} properties',
    commandCount: '{{count}} commands',
    lastUpdated: 'Last Updated',
    createdAt: 'Created At',
    version: 'Version {{version}}',
    author: 'Author: {{author}}',
    noAuthor: 'System Built-in',
    noDescription: 'No description',
    noProperties: 'No property definitions',
    noCommands: 'No command definitions',
  },

  // Marketplace
  marketplace: {
    title: 'Device Template Marketplace',
    description: 'Discover and select from a variety of device templates including',
    and_more: 'and more',
  },
  
  // Empty state
  empty: {
    title: 'No Templates Found',
    description: 'No templates match your current search criteria. Try adjusting your search or filters.',
  },
  
  // Labels
  labels: {
    template: 'Template',
    category: 'Category',
    type: 'Type',
    protocol: 'Protocol',
    manufacturer: 'Manufacturer',
    version: 'Version',
    author: 'Author',
    tags: 'Tags',
  },
  
  // Error messages
  errors: {
    networkError: 'Network connection failed',
    serverError: 'Server error',
    templateNotFound: 'Template not found',
    validationError: 'Input validation failed',
    createError: 'Failed to create device',
    unknownError: 'Unknown error',
  },
  
  // Help information
  help: {
    templateSelection: 'Choose the template that best fits your device. Templates contain basic configuration and functionality definitions for devices',
    deviceConfiguration: 'Fill in device information according to template requirements. Required fields cannot be empty',
    propertyConfiguration: 'Configure default values for device properties. These values will be used after device creation',
    commandSelection: 'Select commands to enable for the device. Required commands will be automatically enabled',
    addressFormat: 'Device address format depends on protocol type, e.g., Modbus uses IP:port, serial port uses COM1, etc.',
    driverOptions: 'Driver options are in JSON format, containing protocol-specific configuration parameters',
  },
}

export default translation