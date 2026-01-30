#!/usr/bin/env node

/**
 * 清理调试代码脚本
 * 移除或注释掉 console.log 等调试语句
 */

const fs = require('fs')
const path = require('path')
const glob = require('glob')

const DRY_RUN = process.argv.includes('--dry-run')
const COMMENT_OUT = process.argv.includes('--comment')

// 需要清理的模式
const DEBUG_PATTERNS = [
  /console\.log\([^)]*\)/g,
  /console\.debug\([^)]*\)/g,
  /console\.info\([^)]*\)/g,
  // 保留 console.warn 和 console.error
]

// 排除的目录
const EXCLUDE_DIRS = ['node_modules', '.next', 'out', 'dist']

// 查找所有 TypeScript/JavaScript 文件
const files = glob.sync('**/*.{ts,tsx,js,jsx}', {
  ignore: EXCLUDE_DIRS.map(dir => `${dir}/**`),
  cwd: path.join(__dirname, '..'),
  absolute: true,
})

let totalFiles = 0
let totalMatches = 0

console.log(`🔍 扫描 ${files.length} 个文件...`)
console.log(`模式: ${DRY_RUN ? '预览模式' : COMMENT_OUT ? '注释模式' : '删除模式'}`)
console.log('')

files.forEach(file => {
  const content = fs.readFileSync(file, 'utf8')
  let newContent = content
  let fileMatches = 0

  DEBUG_PATTERNS.forEach(pattern => {
    const matches = content.match(pattern)
    if (matches) {
      fileMatches += matches.length
      
      if (COMMENT_OUT) {
        // 注释掉而不是删除
        newContent = newContent.replace(pattern, match => `// ${match}`)
      } else {
        // 删除整行
        newContent = newContent.replace(new RegExp(`^.*${pattern.source}.*$`, 'gm'), '')
      }
    }
  })

  if (fileMatches > 0) {
    totalFiles++
    totalMatches += fileMatches
    
    const relativePath = path.relative(path.join(__dirname, '..'), file)
    console.log(`📝 ${relativePath}: ${fileMatches} 处`)

    if (!DRY_RUN) {
      fs.writeFileSync(file, newContent, 'utf8')
    }
  }
})

console.log('')
console.log(`✅ 完成！`)
console.log(`   文件数: ${totalFiles}`)
console.log(`   匹配数: ${totalMatches}`)

if (DRY_RUN) {
  console.log('')
  console.log('💡 这是预览模式，没有修改任何文件')
  console.log('   运行 node scripts/cleanup-debug.js 来实际清理')
  console.log('   运行 node scripts/cleanup-debug.js --comment 来注释而不是删除')
}
