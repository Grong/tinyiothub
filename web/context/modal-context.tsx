'use client'

import type { Dispatch, SetStateAction } from 'react'
import { useCallback, useState } from 'react'
import { createContext, useContext, useContextSelector } from 'use-context-selector'
import AccountSetting from '@/app/components/header/account-setting'
import ApiBasedExtensionModal from '@/app/components/header/account-setting/api-based-extension-page/modal'
import type { PromptVariable } from '@/models/debug'
import type {
  ApiBasedExtension,
} from '@/models/common'
import OpeningSettingModal from '@/app/components/base/features/new-feature-panel/conversation-opener/modal'
import type { OpeningStatement } from '@/app/components/base/features/types'
import { removeSpecificQueryParam } from '@/utils'
import { noop } from 'lodash-es'

export type ModalState<T> = {
  payload: T
  onCancelCallback?: () => void
  onSaveCallback?: (newPayload: T) => void
  onRemoveCallback?: (newPayload: T) => void
  onEditCallback?: (newPayload: T) => void
  onValidateBeforeSaveCallback?: (newPayload: T) => boolean
  isEditMode?: boolean
  datasetBindings?: { id: string; name: string }[]
}

export type ModalContextState = {
  setShowAccountSettingModal: Dispatch<SetStateAction<ModalState<string> | null>>
  setShowApiBasedExtensionModal: Dispatch<SetStateAction<ModalState<ApiBasedExtension> | null>>
  setShowOpeningModal: Dispatch<SetStateAction<ModalState<OpeningStatement & {
    promptVariables?: PromptVariable[]
    onAutoAddPromptVariable?: (variable: PromptVariable[]) => void
  }> | null>>
}
const ModalContext = createContext<ModalContextState>({
  setShowAccountSettingModal: noop,
  setShowApiBasedExtensionModal: noop,
  setShowOpeningModal: noop,
})

export const useModalContext = () => useContext(ModalContext)

// Adding a dangling comma to avoid the generic parsing issue in tsx, see:
// https://github.com/microsoft/TypeScript/issues/15713
export const useModalContextSelector = <T,>(selector: (state: ModalContextState) => T): T =>
  useContextSelector(ModalContext, selector)

type ModalContextProviderProps = {
  children: React.ReactNode
}
export const ModalContextProvider = ({
  children,
}: ModalContextProviderProps) => {
  const [showAccountSettingModal, setShowAccountSettingModal] = useState<ModalState<string> | null>(null)
  const [showApiBasedExtensionModal, setShowApiBasedExtensionModal] = useState<ModalState<ApiBasedExtension> | null>(null)
  const [showOpeningModal, setShowOpeningModal] = useState<ModalState<OpeningStatement & {
    promptVariables?: PromptVariable[]
    onAutoAddPromptVariable?: (variable: PromptVariable[]) => void
  }> | null>(null)
  const handleCancelAccountSettingModal = () => {
    removeSpecificQueryParam('action')
    setShowAccountSettingModal(null)
    if (showAccountSettingModal?.onCancelCallback)
      showAccountSettingModal?.onCancelCallback()
  }
  const handleCancelOpeningModal = useCallback(() => {
    setShowOpeningModal(null)
    if (showOpeningModal?.onCancelCallback)
      showOpeningModal.onCancelCallback()
  }, [showOpeningModal])

  const handleSaveApiBasedExtension = (newApiBasedExtension: ApiBasedExtension) => {
    if (showApiBasedExtensionModal?.onSaveCallback)
      showApiBasedExtensionModal.onSaveCallback(newApiBasedExtension)
    setShowApiBasedExtensionModal(null)
  }

  const handleSaveOpeningModal = (newOpening: OpeningStatement) => {
    if (showOpeningModal?.onSaveCallback)
      showOpeningModal.onSaveCallback(newOpening)
    setShowOpeningModal(null)
  }

  return (
    <ModalContext.Provider value={{
      setShowAccountSettingModal,
      setShowApiBasedExtensionModal,
      setShowOpeningModal,
    }}>
      <>
        {children}
        {
          !!showAccountSettingModal && (
            <AccountSetting
              activeTab={showAccountSettingModal.payload}
              onCancel={handleCancelAccountSettingModal}
            />
          )
        }

        {
          !!showApiBasedExtensionModal && (
            <ApiBasedExtensionModal
              data={showApiBasedExtensionModal.payload}
              onCancel={() => setShowApiBasedExtensionModal(null)}
              onSave={handleSaveApiBasedExtension}
            />
          )
        }
        {showOpeningModal && (
          <OpeningSettingModal
            data={showOpeningModal.payload}
            onSave={handleSaveOpeningModal}
            onCancel={handleCancelOpeningModal}
            promptVariables={showOpeningModal.payload.promptVariables}
            onAutoAddPromptVariable={showOpeningModal.payload.onAutoAddPromptVariable}
          />
        )}
      </>
    </ModalContext.Provider>
  )
}

export default ModalContext
