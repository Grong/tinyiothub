import { useState, useEffect } from 'react'

export type Breakpoint = 'mobile' | 'tablet' | 'pc' | '2k'

export enum MediaType {
  mobile = 'mobile',
  tablet = 'tablet',
  pc = 'pc',
  '2k' = '2k',
}

const breakpoints = {
  mobile: 100,
  tablet: 640,
  pc: 769,
  '2k': 2560,
}

export const useBreakpoints = () => {
  const [currentBreakpoint, setCurrentBreakpoint] = useState<Breakpoint>('pc')

  useEffect(() => {
    // 确保只在客户端执行
    if (typeof window === 'undefined') return

    const updateBreakpoint = () => {
      const width = window.innerWidth
      
      if (width >= breakpoints['2k']) {
        setCurrentBreakpoint('2k')
      } else if (width >= breakpoints.pc) {
        setCurrentBreakpoint('pc')
      } else if (width >= breakpoints.tablet) {
        setCurrentBreakpoint('tablet')
      } else {
        setCurrentBreakpoint('mobile')
      }
    }

    updateBreakpoint()
    window.addEventListener('resize', updateBreakpoint)
    
    return () => window.removeEventListener('resize', updateBreakpoint)
  }, [])

  const isMobile = currentBreakpoint === 'mobile'
  const isTablet = currentBreakpoint === 'tablet'
  const isPc = currentBreakpoint === 'pc'
  const is2k = currentBreakpoint === '2k'

  return {
    currentBreakpoint,
    isMobile,
    isTablet,
    isPc,
    is2k,
  }
}

// Default export for backward compatibility
export default useBreakpoints