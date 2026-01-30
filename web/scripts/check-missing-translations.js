#!/usr/bin/env node

const fs = require('fs')
const path = require('path')
const glob = require('glob')

// 检查文件中是否还有硬编码的英文文本
function checkHardcodedText(filePath) {
  const content = fs.readFileSync(filePath, 'utf8')
  const issues = []
  
  // 检查常见的硬编码文本模式
  const patterns = [
    // JSX 中的硬编码文本
    {
      regex: />\s*[A-Z][a-zA-Z\s]{3,}\s*</g,
      description: 'Possible hardcoded text in JSX'
    },
    // 字符串中的硬编码文本
    {
      regex: /['"`][A-Z][a-zA-Z\s]{5,}['"`]/g,
      description: 'Possible hardcoded text in strings'
    },
    // placeholder 属性
    {
      regex: /placeholder\s*=\s*['"`][A-Z][a-zA-Z\s]{3,}['"`]/g,
      description: 'Hardcoded placeholder text'
    },
    // title 属性
    {
      regex: /title\s*=\s*['"`][A-Z][a-zA-Z\s]{3,}['"`]/g,
      description: 'Hardcoded title text'
    }
  ]
  
  patterns.forEach(pattern => {
    const matches = content.match(pattern.regex)
    if (matches) {
      matches.forEach(match => {
        // 排除一些常见的非翻译文本
        const excludePatterns = [
          /TinyIoTHub/,
          /API/,
          /HTTP/,
          /JSON/,
          /URL/,
          /ID/,
          /UUID/,
          /RGB/,
          /CSS/,
          /HTML/,
          /JavaScript/,
          /TypeScript/,
          /React/,
          /Next\.js/,
          /Tailwind/,
          /className/,
          /useState/,
          /useEffect/,
          /console\./,
          /window\./,
          /document\./,
          /process\./,
          /import/,
          /export/,
          /function/,
          /const/,
          /let/,
          /var/,
          /return/,
          /if/,
          /else/,
          /for/,
          /while/,
          /switch/,
          /case/,
          /break/,
          /continue/,
          /try/,
          /catch/,
          /finally/,
          /throw/,
          /async/,
          /await/,
          /Promise/,
          /Array/,
          /Object/,
          /String/,
          /Number/,
          /Boolean/,
          /Date/,
          /Error/,
          /RegExp/,
          /Math/,
          /JSON/,
          /localStorage/,
          /sessionStorage/,
          /setTimeout/,
          /setInterval/,
          /clearTimeout/,
          /clearInterval/
        ]
        
        const shouldExclude = excludePatterns.some(excludePattern => 
          excludePattern.test(match)
        )
        
        if (!shouldExclude) {
          issues.push({
            type: pattern.description,
            text: match.trim(),
            line: content.substring(0, content.indexOf(match)).split('\n').length
          })
        }
      })
    }
  })
  
  return issues
}

// 检查是否还有未使用命名空间的 useTranslation 调用
function checkMissingNamespaces(filePath) {
  const content = fs.readFileSync(filePath, 'utf8')
  const issues = []
  
  // 检查 useTranslation() 调用
  const useTranslationMatches = content.match(/useTranslation\(\)/g)
  if (useTranslationMatches) {
    issues.push({
      type: 'Missing namespace in useTranslation()',
      count: useTranslationMatches.length
    })
  }
  
  return issues
}

// 主函数
function main() {
  console.log('🔍 Checking for missing translations and hardcoded text...\n')
  
  // 查找所有 TypeScript React 文件
  const files = glob.sync('app/**/*.{ts,tsx}', {
    cwd: path.join(__dirname, '..'),
    absolute: true
  })
  
  let totalIssues = 0
  const fileIssues = []
  
  for (const file of files) {
    try {
      const hardcodedIssues = checkHardcodedText(file)
      const namespaceIssues = checkMissingNamespaces(file)
      const allIssues = [...hardcodedIssues, ...namespaceIssues]
      
      if (allIssues.length > 0) {
        fileIssues.push({
          file: path.relative(path.join(__dirname, '..'), file),
          issues: allIssues
        })
        totalIssues += allIssues.length
      }
    } catch (error) {
      console.error(`❌ Error processing ${file}:`, error.message)
    }
  }
  
  // 输出结果
  if (fileIssues.length === 0) {
    console.log('✅ No obvious translation issues found!')
    console.log('🎉 All files appear to be properly internationalized.')
  } else {
    console.log(`⚠️  Found ${totalIssues} potential translation issues in ${fileIssues.length} files:\n`)
    
    fileIssues.forEach(({ file, issues }) => {
      console.log(`📄 ${file}:`)
      issues.forEach(issue => {
        if (issue.line) {
          console.log(`   Line ${issue.line}: ${issue.type} - "${issue.text}"`)
        } else {
          console.log(`   ${issue.type} (${issue.count} occurrences)`)
        }
      })
      console.log()
    })
    
    console.log('💡 Recommendations:')
    console.log('1. Review the flagged text to determine if it needs translation')
    console.log('2. Extract hardcoded text to appropriate translation files')
    console.log('3. Add missing namespaces to useTranslation() calls')
    console.log('4. Test the application to ensure all text displays correctly')
  }
}

if (require.main === module) {
  main()
}