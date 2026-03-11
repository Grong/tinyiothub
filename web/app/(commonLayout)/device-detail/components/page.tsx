'use client'

import { useParams, useRouter } from 'next/navigation'
import { useEffect } from 'react'

export default function DeviceDetailPage() {
  const params = useParams()
  const router = useRouter()
  const deviceId = params.deviceId as string

  useEffect(() => {
    // Redirect to overview page
    router.replace(`/device/${deviceId}/overview`)
  }, [deviceId, router])

  return null
}
