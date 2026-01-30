'use client'
import React from 'react'
import { useTheme } from 'next-themes'
import { RiMoonLine, RiSunLine } from '@remixicon/react'

const ThemeSwitcher = () => {
  const { theme, setTheme } = useTheme()

  const toggleTheme = () => {
    setTheme(theme === 'dark' ? 'light' : 'dark')
  }

  return (
    <button
      onClick={toggleTheme}
      className="flex items-center justify-center w-8 h-8 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
      aria-label="Toggle theme"
    >
      {theme === 'dark' ? (
        <RiSunLine className="w-4 h-4" />
      ) : (
        <RiMoonLine className="w-4 h-4" />
      )}
    </button>
  )
}

export default ThemeSwitcher