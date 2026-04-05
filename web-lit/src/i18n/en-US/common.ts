const translation = {
  // Navigation
  navigation: {
    dashboard: 'Dashboard',
    devices: 'Devices',
    templates: 'Templates',
    monitoring: 'Monitoring',
    alarms: 'Alarms',
    settings: 'Settings',
    tags: 'Tags',
  },
  
  // Layout
  layout: {
    sidebar: {
      collapseSidebar: 'Collapse Sidebar',
      expandSidebar: 'Expand Sidebar',
    },
  },
  
  // Operations
  operation: {
    more: 'More',
    save: 'Save',
    cancel: 'Cancel',
    confirm: 'Confirm',
    delete: 'Delete',
    edit: 'Edit',
    add: 'Add',
    update: 'Update',
    create: 'Create',
    remove: 'Remove',
    reset: 'Reset',
    refresh: 'Refresh',
    search: 'Search',
    filter: 'Filter',
    export: 'Export',
    import: 'Import',
  },
  
  // Common terms
  optional: 'Optional',
  
  // Messages
  messages: {
    loading: 'Loading...',
    noData: 'No data available',
    error: 'An error occurred',
    success: 'Operation successful',
    confirmDelete: 'Are you sure you want to delete this item?',
    unsavedChanges: 'You have unsaved changes. Are you sure you want to leave?',
    noMembers: 'No team members yet',
    clickAddMembers: 'Click "Add Member" to start inviting team members',
  },

  // Action messages
  actionMsg: {
    fetchFailure: 'Failed to load data',
    searchFailure: 'Search failed',
    createdSuccessfully: 'Created successfully',
    createdUnsuccessfully: 'Creation failed',
    modifiedSuccessfully: 'Updated successfully',
    modifiedUnsuccessfully: 'Update failed',
    deletedSuccessfully: 'Deleted successfully',
    deletedUnsuccessfully: 'Deletion failed',
  },
  
  // Form placeholders
  placeholder: {
    input: 'Please enter',
    search: 'Search...',
    select: 'Please select',
  },
  
  // Pagination
  pagination: {
    total: 'Total {{count}} items',
    page: 'Page',
    pageSize: 'Items per page',
    previous: 'Previous',
    next: 'Next',
    first: 'First',
    last: 'Last',
    goto: 'Go to',
    itemsPerPage: 'items per page',
  },
  
  // Data table
  dataTable: {
    noData: 'No data',
    loading: 'Loading...',
    error: 'Failed to load data',
    retry: 'Retry',
    refresh: 'Refresh',
    columns: 'Columns',
    filters: 'Filters',
    export: 'Export',
    selectAll: 'Select All',
    deselectAll: 'Deselect All',
    selected: '{{count}} selected',
  },
  
  // Language
  language: {
    en: 'English',
    zh: '中文',
    switchLanguage: 'Switch Language',
    currentLanguage: 'Current Language',
    contactAdmin: 'Contact system administrator to add new language support',
    settings: 'Language Settings',
    selectPreferred: 'Select your preferred language, changes will take effect immediately',
    current: 'Current Language',
    selectLanguage: 'Select Language',
    supportNote: 'Language Support Notes',
    immediateEffect: 'Language changes take effect immediately without page refresh',
    technicalTerms: 'Some technical terms may remain in English',
  },
  
  // Tag management
  tag: {
    placeholder: 'Filter by tags',
    addTag: 'Add Tag',
    noTag: 'No tags found',
    manageTags: 'Manage Tags',
    tagDescription: 'Manage and organize tags for your system. Total: {{total}} tags',
    // Tag management
    name: 'Tag Name',
    description: 'Description',
    color: 'Color',
    created: 'Tag created successfully',
    failed: 'Failed to create tag',
    addNew: 'Add New Tag',
    createTag: 'Create Tag',
    editTag: 'Edit Tag',
    deleteTag: 'Delete Tag',
    deleteConfirm: 'Are you sure you want to delete the tag "{{name}}"?',
    deleteTip: 'This action cannot be undone and will remove the tag from all associated items.',
    nameRequired: 'Tag name is required',
    namePlaceholder: 'Enter tag name',
    descriptionPlaceholder: 'Enter tag description (optional)',
    colorPlaceholder: '#6B7280',
    searchPlaceholder: 'Search tags...',
    selectorPlaceholder: 'Search or create tags...',
    create: 'Create',
    noTags: 'No tags available',
    noSearchResults: 'No tags found matching your search',
    tryDifferentSearch: 'Try using different keywords',
    createFirstTag: 'Create your first tag to get started',
    usageCount: '{{count}} uses',
    delete: 'Delete Tag',
  },
  
  // Time
  time: {
    now: 'Now',
    today: 'Today',
    yesterday: 'Yesterday',
    thisWeek: 'This Week',
    thisMonth: 'This Month',
    thisYear: 'This Year',
  },
  
  // App branding
  branding: {
    appName: 'TinyIoTHub',
    appNameFull: 'TinyIoTHub',
  },

  // Pages
  pages: {
    devices: {
      title: 'Device Management',
      subtitle: 'Manage and monitor all IoT devices',
    },
    monitoring: {
      title: 'Monitoring Center',
      subtitle: 'Real-time monitoring of device status and system performance',
      sections: {
        realTimeData: 'Real-time Data Monitoring',
        systemPerformance: 'System Performance',
      },
    },
    alarms: {
      title: 'Alarm Management',
      subtitle: 'Manage and handle system alarm information',
      alarmRules: 'Alarm Rules',
      createRule: 'Create Rule',
    },
    settings: {
      title: 'System Settings',
      subtitle: 'Configure system parameters and user permissions',
      sections: {
        systemConfig: 'System Configuration',
        userManagement: 'User Management',
        networkConfig: 'Network Configuration',
      },
    },
  },

  // Dashboard
  dashboard: {
    title: 'Dashboard',
    welcome: 'Welcome to TinyIoTHub',
    overview: 'System Overview',
    quickActions: 'Quick Actions',
    recentActivity: 'Recent Activity',
    systemStatus: 'System Status',
    deviceSummary: 'Device Summary',
    alarmSummary: 'Alarm Summary',
  },

  // User Profile and Account Management
  userProfile: {
    profile: 'Profile',
    members: 'Members',
    workspace: 'Workspace',
    personalInfo: 'Personal Information',
    username: 'Username',
    email: 'Email',
    phone: 'Phone',
    emailPlaceholder: 'Enter email address',
    phonePlaceholder: 'Enter phone number',
    changePassword: 'Change Password',
    currentPassword: 'Current Password',
    newPassword: 'New Password',
    confirmPassword: 'Confirm Password',
    currentPasswordPlaceholder: 'Enter current password',
    newPasswordPlaceholder: 'Enter new password (at least 6 characters)',
    confirmPasswordPlaceholder: 'Enter new password again',
    accountInfo: 'Account Information',
    userId: 'User ID',
    accountStatus: 'Account Status',
    statusActive: 'Active',
    statusDisabled: 'Disabled',
    lastLogin: 'Last Login',
    parentUser: 'Parent User',
    loadingUserInfo: 'Loading user information...',
    updateSuccess: 'Profile updated successfully',
    updateFailed: 'Failed to update profile',
    updateFailedRetry: 'Update failed, please try again',
    passwordMismatch: 'Passwords do not match',
    passwordTooShort: 'Password must be at least 6 characters',
    passwordChangeSuccess: 'Password changed successfully',
    passwordChangeFailed: 'Failed to change password',
    passwordChangeFailedRetry: 'Password change failed, please try again',
  },

  // Actions
  actions: {
    save: 'Save',
    cancel: 'Cancel',
    confirm: 'Confirm',
    delete: 'Delete',
    edit: 'Edit',
    add: 'Add',
    create: 'Create',
    update: 'Update',
    remove: 'Remove',
    reset: 'Reset',
    refresh: 'Refresh',
    search: 'Search',
    filter: 'Filter',
    export: 'Export',
    import: 'Import',
  },

  // Menus
  menus: {
    tools: 'Tools',
    explore: 'Explore',
  },

  // Navigation items
  nav: {
    dashboard: 'Dashboard',
    devices: 'Devices',
    monitoring: 'Monitoring',
    tags: 'Tags',
  },

  // Device menus
  deviceMenus: {
    overview: 'Overview',
    events: 'Events',
    monitoring: 'Monitoring',
    configuration: 'Configuration',
  },
}

export default translation