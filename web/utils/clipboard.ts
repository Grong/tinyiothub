import copyToClipboard from 'copy-to-clipboard'

export const copyText = (text: string): boolean => {
  return copyToClipboard(text)
}

export const copyToClipboardAsync = async (text: string): Promise<boolean> => {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text)
      return true
    } else {
      return copyToClipboard(text)
    }
  } catch {
    return copyToClipboard(text)
  }
}