'use client'
import { useEffect } from 'react'

export default function useDocumentTitle(title: string) {
  useEffect(() => {
    const prefix = title ? `${title} - ` : ''
    const titleStr = `${prefix}TinyIoTHub`
    document.title = titleStr
  }, [title])
}