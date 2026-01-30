#!/usr/bin/env node

const fs = require('fs')
const path = require('path')

// 获取所有支持的语言
function getSupportedLocales() {
  const configPath = path.join(__dirname, 'index.ts')
  const configContent = fs.readFileSync(configPath, 'utf8')
  
  // 提取 locales 数组
  const localesMatch = configContent.match(/locales:\s*\[(.*?)\]/s)
  if (!localesMatch) {
    throw new Error('Could not find locales array in index.ts')
  }
  
  const localesStr = localesMatch[1]
  const locales = localesStr
    .split(',')
    .map(line => line.trim())
    .filter(line => line.startsWith("'") || line.startsWith('"'))
    .map(line => line.slice(1, -1)) // 移除引号
  
  return locales
}

// 获取指定语言的所有翻译文件
function getTranslationFiles(locale) {
  const localeDir = path.join(__dirname, '../i18n', locale)
  
  if (!fs.existsSync(localeDir)) {
    return []
  }
  
  return fs.readdirSync(localeDir)
    .filter(file => file.endsWith('.ts'))
    .map(file => file.replace('.ts', ''))
}

// 递归获取对象的所有键路径
function getObjectKeys(obj, prefix = '') {
  const keys = []
  
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key
    
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      keys.push(...getObjectKeys(value, fullKey))
    } else {
      keys.push(fullKey)
    }
  }
  
  return keys
}

// 加载翻译文件并获取所有键
function getTranslationKeys(locale, namespace) {
  try {
    const filePath = path.join(__dirname, '../i18n', locale, `${namespace}.ts`)
    
    if (!fs.existsSync(filePath)) {
      return null
    }
    
    // 动态导入翻译文件
    delete require.cache[require.resolve(filePath)]
    const translation = require(filePath).default
    
    return getObjectKeys(translation).sort()
  } catch (error) {
    console.error(`Error loading ${locale}/${namespace}.ts:`, error.message)
    return null
  }
}

// 比较两个键数组
function compareKeys(keys1, keys2) {
  const set1 = new Set(keys1 || [])
  const set2 = new Set(keys2 || [])
  
  const missing = [...set1].filter(key => !set2.has(key))
  const extra = [...set2].filter(key => !set1.has(key))
  
  return { missing, extra }
}

function main() {
  try {
    console.log('🔍 Checking i18n synchronization...\n')
    
    // 获取支持的语言
    const locales = getSupportedLocales()
    console.log(`📦 Supported locales: ${locales.join(', ')}`)
    
    // 获取所有翻译文件（以第一个语言为基准）
    const baseLocale = locales[0]
    const namespaces = getTranslationFiles(baseLocale)
    console.log(`📄 Found ${namespaces.length} translation files: ${namespaces.join(', ')}\n`)
    
    let hasErrors = false
    
    // 检查每个命名空间
    for (const namespace of namespaces) {
      console.log(`🔍 Checking namespace: ${namespace}`)
      
      // 检查所有语言是否都有这个文件
      const missingFiles = []
      for (const locale of locales) {
        const files = getTranslationFiles(locale)
        if (!files.includes(namespace)) {
          missingFiles.push(locale)
        }
      }
      
      if (missingFiles.length > 0) {
        hasErrors = true
        console.error(`  ❌ Missing file in locales: ${missingFiles.join(', ')}`)
        continue
      }
      
      // 获取基准语言的键
      const baseKeys = getTranslationKeys(baseLocale, namespace)
      if (!baseKeys) {
        hasErrors = true
        console.error(`  ❌ Failed to load base translation for ${baseLocale}/${namespace}`)
        continue
      }
      
      // 与其他语言比较
      for (const locale of locales.slice(1)) {
        const keys = getTranslationKeys(locale, namespace)
        if (!keys) {
          hasErrors = true
          console.error(`  ❌ Failed to load translation for ${locale}/${namespace}`)
          continue
        }
        
        const { missing, extra } = compareKeys(baseKeys, keys)
        
        if (missing.length > 0) {
          hasErrors = true
          console.error(`  ❌ Missing keys in ${locale}:`)
          missing.forEach(key => console.error(`     - ${key}`))
        }
        
        if (extra.length > 0) {
          hasErrors = true
          console.error(`  ❌ Extra keys in ${locale}:`)
          extra.forEach(key => console.error(`     - ${key}`))
        }
        
        if (missing.length === 0 && extra.length === 0) {
          console.log(`  ✅ ${locale} is synchronized`)
        }
      }
      
      console.log()
    }
    
    if (hasErrors) {
      console.error('💡 To fix synchronization issues:')
      console.error('   1. Add missing translation files')
      console.error('   2. Add missing translation keys')
      console.error('   3. Remove extra translation keys')
      console.error('   4. Follow the structure in web/docs/I18N_GUIDELINES.md')
      process.exit(1)
    }
    
    console.log('✅ All i18n files are synchronized!')
    
  } catch (error) {
    console.error('❌ Error:', error.message)
    process.exit(1)
  }
}

if (require.main === module) {
  main()
}