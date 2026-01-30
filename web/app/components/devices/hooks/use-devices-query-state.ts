import { useCallback } from 'react'
import { useSearchParams, useRouter } from 'next/navigation'

export type DevicesQueryState = {
  tagIDs: string[]
  keywords: string
  isCreatedByMe: boolean
}

const useDevicesQueryState = () => {
  const router = useRouter()
  const searchParams = useSearchParams()

  const query: DevicesQueryState = {
    tagIDs: searchParams.get('tagIDs')?.split(',').filter(Boolean) || [],
    keywords: searchParams.get('keywords') || '',
    isCreatedByMe: searchParams.get('isCreatedByMe') === 'true',
  }

  const setQuery = useCallback((newQuery: Partial<DevicesQueryState> | ((prev: DevicesQueryState) => Partial<DevicesQueryState>)) => {
    const currentQuery = {
      tagIDs: searchParams.get('tagIDs')?.split(',').filter(Boolean) || [],
      keywords: searchParams.get('keywords') || '',
      isCreatedByMe: searchParams.get('isCreatedByMe') === 'true',
    }

    const updatedQuery = typeof newQuery === 'function' ? newQuery(currentQuery) : newQuery
    const finalQuery = { ...currentQuery, ...updatedQuery }

    const params = new URLSearchParams()
    
    if (finalQuery.tagIDs && finalQuery.tagIDs.length > 0) {
      params.set('tagIDs', finalQuery.tagIDs.join(','))
    }
    
    if (finalQuery.keywords) {
      params.set('keywords', finalQuery.keywords)
    }
    
    if (finalQuery.isCreatedByMe) {
      params.set('isCreatedByMe', 'true')
    }

    const queryString = params.toString()
    const newUrl = queryString ? `?${queryString}` : ''
    
    router.replace(newUrl, { scroll: false })
  }, [router, searchParams])

  return { query, setQuery }
}

export default useDevicesQueryState