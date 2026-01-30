'use client'

import React from 'react'
import { DriverMarketplaceProvider } from './context'
import Description from './description'
import SearchBox from './search-box'
import ProtocolFilter from './protocol-filter'
import DriverListWrapper from './driver-list-wrapper'

const DriverMarketplace: React.FC = () => {
  return (
    <DriverMarketplaceProvider>
      <Description />
      <div className="sticky top-[60px] z-10 mt-4 bg-background-body">
        <SearchBox />
        <ProtocolFilter />
      </div>
      <DriverListWrapper />
    </DriverMarketplaceProvider>
  )
}

export default DriverMarketplace
