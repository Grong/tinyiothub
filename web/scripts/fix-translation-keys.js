#!/usr/bin/env node

const fs = require('fs')
const path = require('path')
const glob = require('glob')

// 翻译键映射表
const keyMappings = {
  // 通用操作
  "t('common.actions.save')": "t('actions.save')",
  "t('common.actions.cancel')": "t('actions.cancel')",
  "t('common.actions.confirm')": "t('actions.confirm')",
  "t('common.actions.delete')": "t('actions.delete')",
  "t('common.actions.edit')": "t('actions.edit')",
  "t('common.actions.add')": "t('actions.add')",
  "t('common.actions.create')": "t('actions.create')",
  "t('common.actions.update')": "t('actions.update')",
  
  // 导航
  "t('common.nav.dashboard')": "t('nav.dashboard')",
  "t('common.nav.devices')": "t('nav.devices')",
  "t('common.nav.monitoring')": "t('nav.monitoring')",
  "t('common.nav.tags')": "t('nav.tags')",
  
  // 菜单
  "t('common.menus.tools')": "t('menus.tools')",
  "t('common.menus.explore')": "t('menus.explore')",
  
  // 用户资料
  "t('common.userProfile.profile')": "t('userProfile.profile')",
  "t('common.userProfile.members')": "t('userProfile.members')",
  "t('common.userProfile.workspace')": "t('userProfile.workspace')",
  "t('common.userProfile.personalInfo')": "t('userProfile.personalInfo')",
  "t('common.userProfile.username')": "t('userProfile.username')",
  "t('common.userProfile.email')": "t('userProfile.email')",
  "t('common.userProfile.phone')": "t('userProfile.phone')",
  "t('common.userProfile.emailPlaceholder')": "t('userProfile.emailPlaceholder')",
  "t('common.userProfile.phonePlaceholder')": "t('userProfile.phonePlaceholder')",
  "t('common.userProfile.changePassword')": "t('userProfile.changePassword')",
  "t('common.userProfile.currentPassword')": "t('userProfile.currentPassword')",
  "t('common.userProfile.newPassword')": "t('userProfile.newPassword')",
  "t('common.userProfile.confirmPassword')": "t('userProfile.confirmPassword')",
  "t('common.userProfile.currentPasswordPlaceholder')": "t('userProfile.currentPasswordPlaceholder')",
  "t('common.userProfile.newPasswordPlaceholder')": "t('userProfile.newPasswordPlaceholder')",
  "t('common.userProfile.confirmPasswordPlaceholder')": "t('userProfile.confirmPasswordPlaceholder')",
  "t('common.userProfile.accountInfo')": "t('userProfile.accountInfo')",
  "t('common.userProfile.userId')": "t('userProfile.userId')",
  "t('common.userProfile.accountStatus')": "t('userProfile.accountStatus')",
  "t('common.userProfile.statusActive')": "t('userProfile.statusActive')",
  "t('common.userProfile.statusDisabled')": "t('userProfile.statusDisabled')",
  "t('common.userProfile.lastLogin')": "t('userProfile.lastLogin')",
  "t('common.userProfile.parentUser')": "t('userProfile.parentUser')",
  "t('common.userProfile.loadingUserInfo')": "t('userProfile.loadingUserInfo')",
  "t('common.userProfile.updateSuccess')": "t('userProfile.updateSuccess')",
  "t('common.userProfile.updateFailed')": "t('userProfile.updateFailed')",
  "t('common.userProfile.updateFailedRetry')": "t('userProfile.updateFailedRetry')",
  "t('common.userProfile.passwordMismatch')": "t('userProfile.passwordMismatch')",
  "t('common.userProfile.passwordTooShort')": "t('userProfile.passwordTooShort')",
  "t('common.userProfile.passwordChangeSuccess')": "t('userProfile.passwordChangeSuccess')",
  "t('common.userProfile.passwordChangeFailed')": "t('userProfile.passwordChangeFailed')",
  "t('common.userProfile.passwordChangeFailedRetry')": "t('userProfile.passwordChangeFailedRetry')",
  
  // 语言设置
  "t('common.language.settings')": "t('language.settings')",
  "t('common.language.selectPreferred')": "t('language.selectPreferred')",
  "t('common.language.current')": "t('language.current')",
  "t('common.language.selectLanguage')": "t('language.selectLanguage')",
  "t('common.language.supportNote')": "t('language.supportNote')",
  "t('common.language.immediateEffect')": "t('language.immediateEffect')",
  "t('common.language.technicalTerms')": "t('language.technicalTerms')",
  "t('common.language.contactAdmin')": "t('language.contactAdmin')",
}

function fixTranslationKeys(filePath) {
  let content = fs.readFileSync(filePath, 'utf8')
  let modified = false
  
  // 应用所有映射
  for (const [oldKey, newKey] of Object.entries(keyMappings)) {
    if (content.includes(oldKey)) {
      content = content.replace(new RegExp(oldKey.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'), newKey)
      modified = true
    }
  }
  
  if (modified) {
    fs.writeFileSync(filePath, content, 'utf8')
    console.log(`✅ Fixed: ${filePath}`)
    return true
  }
  
  return false
}

function main() {
  console.log('🔧 Fixing translation keys...\n')
  
  // 查找所有 TypeScript React 文件
  const files = glob.sync('app/**/*.{ts,tsx}', {
    cwd: path.join(__dirname, '..'),
    absolute: true
  })
  
  let fixedCount = 0
  
  for (const file of files) {
    try {
      if (fixTranslationKeys(file)) {
        fixedCount++
      }
    } catch (error) {
      console.error(`❌ Error processing ${file}:`, error.message)
    }
  }
  
  console.log(`\n🎉 Fixed ${fixedCount} files`)
}

if (require.main === module) {
  main()
}