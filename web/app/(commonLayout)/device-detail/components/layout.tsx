'use client'

import { useParams } from 'next/navigation'
import Main from './layout-main'

const DeviceDetailLayout = ({ children }: { children: React.ReactNode }) => {
  const params = useParams()
  const deviceId = params.deviceId as string

  return <Main deviceId={deviceId}>{children}</Main>
}

export default DeviceDetailLayout
