const fs = require('fs')
const path = require('path')

const standaloneDir = path.join(__dirname, '../.next/standalone')
const staticDir = path.join(__dirname, '../.next/static')
const publicDir = path.join(__dirname, '../public')

function copyRecursiveSync(src, dest) {
  const exists = fs.existsSync(src)
  const stats = exists && fs.statSync(src)
  const isDirectory = exists && stats.isDirectory()
  
  if (isDirectory) {
    if (!fs.existsSync(dest)) {
      fs.mkdirSync(dest, { recursive: true })
    }
    fs.readdirSync(src).forEach(childItemName => {
      copyRecursiveSync(
        path.join(src, childItemName),
        path.join(dest, childItemName)
      )
    })
  } else {
    fs.copyFileSync(src, dest)
  }
}

function optimizeStandalone() {
  console.log('🚀 Optimizing standalone build...')
  
  // Copy static files
  if (fs.existsSync(staticDir)) {
    const targetStaticDir = path.join(standaloneDir, '.next/static')
    copyRecursiveSync(staticDir, targetStaticDir)
    console.log('✅ Copied static files')
  }
  
  // Copy public files
  if (fs.existsSync(publicDir)) {
    const targetPublicDir = path.join(standaloneDir, 'public')
    copyRecursiveSync(publicDir, targetPublicDir)
    console.log('✅ Copied public files')
  }
  
  console.log('🎉 Standalone build optimization complete!')
}

optimizeStandalone()