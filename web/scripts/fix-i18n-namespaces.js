#!/usr/bin/env node

const fs = require('fs')
const path = require('path')
const glob = require('glob')

// 定义命名空间映射规则
const namespaceRules = [
  // 设备相关组件
  {
    pattern: /\/devices\//,
    namespace: 'device'
  },
  // 登录相关组件
  {
    pattern: /\/signin\//,
    namespace: 'login'
  },
  // 布局和导航相关组件
  {
    pattern: /\/(header|sidebar|layout|nav)\//,
    namespace: 'layout'
  },
  // 标签管理页面
  {
    pattern: /\/tags\/page\.tsx$/,
    namespace: 'common' // 标签管理页面使用 common 命名空间
  },
  // 其他所有组件默认使用 common
  {
    pattern: /.*/,
    namespace: 'common'
  }
]

// 获取文件应该使用的命名空间
function getNamespaceForFile(filePath) {
  for (const rule of namespaceRules) {
    if (rule.pattern.test(filePath)) {
      return rule.namespace
    }
  }
  return 'common'
}

// 修复文件中的 useTranslation 调用
function fixTranslationInFile(filePath) {
  const content = fs.readFileSync(filePath, 'utf8')
  
  // 检查是否包含 useTranslation()
  if (!content.includes('useTranslation()')) {
    return false
  }
  
  const namespace = getNamespaceForFile(filePath)
  
  // 替换 useTranslation() 为 useTranslation('namespace')
  const updatedContent = content.replace(
    /useTranslation\(\)/g,
    `useTranslation('${namespace}')`
  )
  
  // 如果内容有变化，写回文件
  if (updatedContent !== content) {
    fs.writeFileSync(filePath, updatedContent, 'utf8')
    console.log(`✅ Fixed: ${filePath} -> namespace: ${namespace}`)
    return true
  }
  
  return false
}

// 主函数
function main() {
  console.log('🔧 Fixing i18n namespaces...\n')
  
  // 查找所有 TypeScript React 文件
  const files = glob.sync('app/**/*.{ts,tsx}', {
    cwd: path.join(__dirname, '..'),
    absolute: true
  })
  
  let fixedCount = 0
  
  for (const file of files) {
    try {
      if (fixTranslationInFile(file)) {
        fixedCount++
      }
    } catch (error) {
      console.error(`❌ Error processing ${file}:`, error.message)
    }
  }
  
  console.log(`\n🎉 Fixed ${fixedCount} files`)
  
  if (fixedCount > 0) {
    console.log('\n📋 Next steps:')
    console.log('1. Review the changes')
    console.log('2. Run: npm run check:i18n-sync')
    console.log('3. Fix any remaining translation key issues')
    console.log('4. Test the application')
  }
}

if (require.main === module) {
  main()
}