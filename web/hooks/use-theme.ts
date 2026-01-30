'use client'
import { useTheme as useNextTheme } from 'next-themes'

const useTheme = () => {
  const { theme, setTheme, systemTheme } = useNextTheme()
  
  return {
    theme: theme === 'system' ? systemTheme : theme,
    setTheme,
    systemTheme,
  }
}

export default useTheme