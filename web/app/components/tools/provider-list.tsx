'use client'
import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import cn from '@/utils/classnames'
import { useTabSearchParams } from '@/hooks/use-tab-searchparams'
import TabSliderNew from '@/app/components/base/tab-slider-new'
import LabelFilter from '@/app/components/tools/labels/filter'
import Input from '@/app/components/base/input'
import { useGlobalPublicStore } from '@/context/global-public-context'

const ProviderList = () => {
  const { t } = useTranslation()
  const { enable_marketplace } = useGlobalPublicStore(s => s.systemFeatures)
  const containerRef = useRef<HTMLDivElement>(null)

  const [activeTab, setActiveTab] = useTabSearchParams({
    defaultTab: 'builtin',
  })
  const options = [
    { value: 'builtin', text: t('tools.type.builtIn') },
    { value: 'api', text: t('tools.type.custom') },
  ]
  const [tagFilterValue, setTagFilterValue] = useState<string[]>([])
  const handleTagsChange = (value: string[]) => {
    setTagFilterValue(value)
  }
  const [keywords, setKeywords] = useState<string>('')
  const handleKeywordsChange = (value: string) => {
    setKeywords(value)
  }

  return (
    <>
      <div className='relative flex h-0 shrink-0 grow overflow-hidden'>
        <div
          ref={containerRef}
          className='relative flex grow flex-col overflow-y-auto bg-background-body'
        >
          <div className={cn(
            'sticky top-0 z-20 flex flex-wrap items-center justify-between gap-y-2 bg-background-body px-12 pb-2 pt-4 leading-[56px]',
          )}>
            <TabSliderNew
              value={activeTab}
              onChange={(state) => {
                setActiveTab(state)
              }}
              options={options}
            />
            <div className='flex items-center gap-2'>
              <LabelFilter value={tagFilterValue} onChange={handleTagsChange} />
              <Input
                showLeftIcon
                showClearIcon
                wrapperClassName='w-[200px]'
                value={keywords}
                onChange={e => handleKeywordsChange(e.target.value)}
                onClear={() => handleKeywordsChange('')}
              />
            </div>
          </div>
          {/* {activeTab === 'builtin' && (
            <Empty lightCard text={t('tools.noTools')} className='h-[224px] px-12' />
          )} */}
          {/* {
            enable_marketplace && activeTab === 'builtin' && (
              <Marketplace
                onMarketplaceScroll={() => {
                  containerRef.current?.scrollTo({ top: containerRef.current.scrollHeight, behavior: 'smooth' })
                }}
                searchPluginText={keywords}
                filterPluginTags={tagFilterValue}
              />
            )
          } */}
        </div >
      </div >
      {/* <PluginDetailPanel
        detail={currentPluginDetail}
        onUpdate={() => invalidateInstalledPluginList()}
        onHide={() => setCurrentProviderId(undefined)}
      /> */}
    </>
  )
}
ProviderList.displayName = 'ToolProviderList'
export default ProviderList
