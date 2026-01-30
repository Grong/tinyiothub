const fs = require('fs')
const path = require('path')

const i18nDir = path.join(__dirname, '../i18n')
const languages = ['en-US', 'zh-Hans']

function checkI18nFiles() {
  const errors = []
  
  // Check if all language directories exist
  languages.forEach(lang => {
    const langDir = path.join(i18nDir, lang)
    if (!fs.existsSync(langDir)) {
      errors.push(`Missing language directory: ${lang}`)
      return
    }
    
    // Check if common.ts exists
    const commonFile = path.join(langDir, 'common.ts')
    if (!fs.existsSync(commonFile)) {
      errors.push(`Missing common.ts file for language: ${lang}`)
    }
  })
  
  if (errors.length > 0) {
    console.error('❌ i18n validation failed:')
    errors.forEach(error => console.error(`  - ${error}`))
    process.exit(1)
  } else {
    console.log('✅ i18n validation passed')
  }
}

checkI18nFiles()