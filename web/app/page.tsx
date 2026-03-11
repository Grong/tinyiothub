'use client'

import { useEffect } from 'react'
import Loading from '@/app/components/base/loading'

const Home = () => {
  useEffect(() => {
    // 静态导出模式下，使用 window.location 强制跳转
    window.location.href = '/signin'
  }, [])

  return (
    <div className="flex min-h-screen flex-col justify-center py-12 sm:px-6 lg:px-8">
      <div className="sm:mx-auto sm:w-full sm:max-w-md">
        <Loading type='area' />
        <div className="mt-10 text-center text-gray-600">
          Redirecting to login...
        </div>
      </div>
    </div>
  )
}

export default Home