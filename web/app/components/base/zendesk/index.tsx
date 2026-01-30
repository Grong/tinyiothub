'use client'

import { memo } from 'react'
import Script from 'next/script'
import { IS_CE_EDITION, ZENDESK_WIDGET_KEY } from '@/config'

const Zendesk = () => {
  if (IS_CE_EDITION || !ZENDESK_WIDGET_KEY)
    return null

  return (
    <>
      <Script
        id="ze-snippet"
        src={`https://static.zdassets.com/ekr/snippet.js?key=${ZENDESK_WIDGET_KEY}`}
      />
      <Script id="ze-init">{`
        (function () {
          window.addEventListener('load', function () {
            if (window.zE)
              window.zE('messenger', 'hide')
          })
        })()
      `}</Script>
    </>
  )
}

export default memo(Zendesk)
