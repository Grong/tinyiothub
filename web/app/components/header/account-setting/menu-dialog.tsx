import { Fragment, useCallback, useEffect } from 'react'
import type { ReactNode } from 'react'
import { Dialog, DialogPanel, Transition, TransitionChild } from '@headlessui/react'
import cn from '@/utils/classnames'
import { noop } from 'lodash-es'

type DialogProps = {
  className?: string
  children: ReactNode
  show: boolean
  onClose?: () => void
}

const MenuDialog = ({
  className,
  children,
  show,
  onClose,
}: DialogProps) => {
  const close = useCallback(() => onClose?.(), [onClose])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        close()
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [close])

  return (
    <Transition appear show={show} as={Fragment}>
      <Dialog as="div" className="relative z-[60]" onClose={noop}>
        <TransitionChild
          as={Fragment}
          enter="ease-out duration-300"
          enterFrom="opacity-0"
          enterTo="opacity-100"
          leave="ease-in duration-200"
          leaveFrom="opacity-100"
          leaveTo="opacity-0"
        >
          <div className="fixed inset-0 bg-black/25 backdrop-blur-sm" />
        </TransitionChild>

        <div className="fixed inset-0 overflow-y-auto">
          <div className="flex min-h-full items-center justify-center p-4">
            <TransitionChild
              as={Fragment}
              enter="ease-out duration-300"
              enterFrom="opacity-0 scale-95"
              enterTo="opacity-100 scale-100"
              leave="ease-in duration-200"
              leaveFrom="opacity-100 scale-100"
              leaveTo="opacity-0 scale-95"
            >
              <DialogPanel className={cn(
                'relative w-full max-w-[1048px] h-[640px] overflow-hidden bg-components-panel-bg rounded-2xl shadow-2xl border border-divider-subtle transition-all',
                className,
              )}>
                {children}
              </DialogPanel>
            </TransitionChild>
          </div>
        </div>
      </Dialog>
    </Transition >
  )
}

export default MenuDialog