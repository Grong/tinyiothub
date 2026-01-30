const translation = {
  // Navigation
  navigation: {
    dashboard: '仪表板',
    devices: '设备',
    templates: '模板',
    monitoring: '监控中心',
    alarms: '告警',
    settings: '系统设置',
    tags: '标签',
  },
  
  // Layout
  layout: {
    sidebar: {
      collapseSidebar: '收起侧边栏',
      expandSidebar: '展开侧边栏',
    },
  },
  
  // Operations
  operation: {
    more: '更多',
    save: '保存',
    cancel: '取消',
    confirm: '确认',
    delete: '删除',
    edit: '编辑',
    add: '添加',
    update: '更新',
    create: '创建',
    remove: '移除',
    reset: '重置',
    refresh: '刷新',
    search: '搜索',
    filter: '筛选',
    export: '导出',
    import: '导入',
  },
  
  // Common terms
  optional: '可选',
  
  // Messages
  messages: {
    loading: '加载中...',
    noData: '暂无数据',
    error: '发生错误',
    success: '操作成功',
    confirmDelete: '确定要删除此项吗？',
    unsavedChanges: '您有未保存的更改。确定要离开吗？',
    noMembers: '暂无团队成员',
    clickAddMembers: '点击"添加成员"开始邀请团队成员',
  },

  // Action messages
  actionMsg: {
    fetchFailure: '数据加载失败',
    searchFailure: '搜索失败',
    createdSuccessfully: '创建成功',
    createdUnsuccessfully: '创建失败',
    modifiedSuccessfully: '更新成功',
    modifiedUnsuccessfully: '更新失败',
    deletedSuccessfully: '删除成功',
    deletedUnsuccessfully: '删除失败',
  },
  
  // Form placeholders
  placeholder: {
    input: '请输入',
    search: '搜索...',
    select: '请选择',
  },
  
  // Pagination
  pagination: {
    total: '共 {{count}} 项',
    page: '页',
    pageSize: '每页条数',
    previous: '上一页',
    next: '下一页',
    first: '首页',
    last: '末页',
    goto: '跳转到',
    itemsPerPage: '条/页',
  },
  
  // Data table
  dataTable: {
    noData: '暂无数据',
    loading: '加载中...',
    error: '数据加载失败',
    retry: '重试',
    refresh: '刷新',
    columns: '列',
    filters: '筛选',
    export: '导出',
    selectAll: '全选',
    deselectAll: '取消全选',
    selected: '已选择 {{count}} 项',
  },
  
  // Language
  language: {
    en: 'English',
    zh: '中文',
    switchLanguage: '切换语言',
    currentLanguage: '当前语言',
    contactAdmin: '如需添加新语言支持，请联系系统管理员',
    settings: '语言设置',
    selectPreferred: '选择您偏好的语言，更改将立即生效',
    current: '当前语言',
    selectLanguage: '选择语言',
    supportNote: '语言支持说明',
    immediateEffect: '语言更改将立即生效，无需刷新页面',
    technicalTerms: '部分技术术语可能保持英文显示',
  },
  
  // Tag management
  tag: {
    placeholder: '按标签筛选',
    addTag: '添加标签',
    noTag: '未找到标签',
    manageTags: '标签管理',
    tagDescription: '管理和组织系统标签。总计：{{total}} 个标签',
    // 标签管理
    name: '标签名称',
    description: '描述',
    color: '颜色',
    created: '标签创建成功',
    failed: '标签创建失败',
    addNew: '添加新标签',
    createTag: '创建标签',
    editTag: '编辑标签',
    deleteTag: '删除标签',
    deleteConfirm: '确定要删除标签"{{name}}"吗？',
    deleteTip: '此操作无法撤销，将从所有关联项目中移除该标签。',
    nameRequired: '标签名称为必填项',
    namePlaceholder: '请输入标签名称',
    descriptionPlaceholder: '请输入标签描述（可选）',
    colorPlaceholder: '#6B7280',
    searchPlaceholder: '搜索标签...',
    selectorPlaceholder: '搜索或创建标签...',
    create: '创建',
    noTags: '暂无标签',
    noSearchResults: '未找到匹配的标签',
    tryDifferentSearch: '请尝试使用不同的关键词',
    createFirstTag: '创建您的第一个标签',
    usageCount: '{{count}} 次使用',
    delete: '删除标签',
  },
  
  // Time
  time: {
    now: '现在',
    today: '今天',
    yesterday: '昨天',
    thisWeek: '本周',
    thisMonth: '本月',
    thisYear: '今年',
  },
  
  // App branding
  branding: {
    appName: 'TinyIoTHub',
    appNameFull: 'TinyIoTHub',
  },

  // Pages
  pages: {
    devices: {
      title: '设备管理',
      subtitle: '管理和监控所有物联网设备',
    },
    monitoring: {
      title: '监控中心',
      subtitle: '实时监控设备状态和系统性能',
      sections: {
        realTimeData: '实时数据监控',
        systemPerformance: '系统性能',
      },
    },
    alarms: {
      title: '告警管理',
      subtitle: '管理和处理系统告警信息',
      alarmRules: '告警规则',
      createRule: '创建规则',
    },
    settings: {
      title: '系统设置',
      subtitle: '配置系统参数和用户权限',
      sections: {
        systemConfig: '系统配置',
        userManagement: '用户管理',
        networkConfig: '网络配置',
      },
    },
  },

  // Dashboard
  dashboard: {
    title: '仪表板',
    welcome: '欢迎使用 TinyIoTHub',
    overview: '系统概览',
    quickActions: '快捷操作',
    recentActivity: '最近活动',
    systemStatus: '系统状态',
    deviceSummary: '设备概况',
    alarmSummary: '告警概况',
  },

  // User Profile and Account Management
  userProfile: {
    profile: '个人资料',
    members: '成员管理',
    workspace: '工作空间',
    personalInfo: '个人信息',
    username: '用户名',
    email: '邮箱',
    phone: '电话',
    emailPlaceholder: '请输入邮箱地址',
    phonePlaceholder: '请输入电话号码',
    changePassword: '修改密码',
    currentPassword: '当前密码',
    newPassword: '新密码',
    confirmPassword: '确认密码',
    currentPasswordPlaceholder: '请输入当前密码',
    newPasswordPlaceholder: '请输入新密码（至少6位）',
    confirmPasswordPlaceholder: '请再次输入新密码',
    accountInfo: '账户信息',
    userId: '用户ID',
    accountStatus: '账户状态',
    statusActive: '正常',
    statusDisabled: '已禁用',
    lastLogin: '最后登录',
    parentUser: '上级用户',
    loadingUserInfo: '正在加载用户信息...',
    updateSuccess: '个人信息更新成功',
    updateFailed: '个人信息更新失败',
    updateFailedRetry: '更新失败，请重试',
    passwordMismatch: '两次输入的密码不一致',
    passwordTooShort: '密码长度至少6位',
    passwordChangeSuccess: '密码修改成功',
    passwordChangeFailed: '密码修改失败',
    passwordChangeFailedRetry: '密码修改失败，请重试',
    settings: '设置',
    logout: '退出登录',
  },

  // Account management
  account: {
    account: '账户',
    profile: '个人资料',
    settings: '账户设置',
    security: '安全设置',
    preferences: '偏好设置',
  },

  // Theme management
  theme: {
    theme: '主题',
    light: '浅色主题',
    dark: '深色主题',
    system: '跟随系统',
    switchTheme: '切换主题',
  },

  // Actions
  actions: {
    save: '保存',
    cancel: '取消',
    confirm: '确认',
    delete: '删除',
    edit: '编辑',
    add: '添加',
    create: '创建',
    update: '更新',
    remove: '移除',
    reset: '重置',
    refresh: '刷新',
    search: '搜索',
    filter: '筛选',
    export: '导出',
    import: '导入',
  },

  // Menus
  menus: {
    tools: '工具',
    explore: '探索',
  },

  // Navigation items
  nav: {
    dashboard: '仪表板',
    devices: '设备管理',
    monitoring: '监控中心',
    tags: '标签管理',
  },

  // Device menus
  deviceMenus: {
    overview: '概览',
    events: '事件',
    monitoring: '监控',
    configuration: '配置',
  },
}

export default translation