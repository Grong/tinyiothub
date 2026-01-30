'use client'

import { useState, useEffect } from 'react'
import { createPortal } from 'react-dom'
import { XMarkIcon } from '@heroicons/react/24/outline'
import { CheckCircleIcon, ExclamationTriangleIcon, InformationCircleIcon, XCircleIcon } from '@heroicons/react/24/solid'
import cn from '@/utils/classnames'

export interface ToastProps {
  id: string
  type: 'success' | 'error' | 'warning' | 'info'
  message: string
  duration?: number
  onClose: (id: string) => void
}

const Toast = ({ id, type, message, duration = 3000, onClose }: ToastProps) => {
  const [isVisible, setIsVisible] = useState(true)

  useEffect(() => {
    const timer = setTimeout(() => {
      setIsVisible(false)
      setTimeout(() => onClose(id), 300) // Wait for animation
    }, duration)

    return () => clearTimeout(timer)
  }, [id, duration, onClose])

  const handleClose = () => {
    setIsVisible(false)
    setTimeout(() => onClose(id), 300)
  }

  const getIcon = () => {
    switch (type) {
      case 'success':
        return <CheckCircleIcon className="w-5 h-5 text-text-success" />
      case 'error':
        return <XCircleIcon className="w-5 h-5 text-text-destructive" />
      case 'warning':
        return <ExclamationTriangleIcon className="w-5 h-5 text-text-warning" />
      case 'info':
        return <InformationCircleIcon className="w-5 h-5 text-text-accent" />
    }
  }

  const getBackgroundColor = () => {
    switch (type) {
      case 'success':
        return 'bg-components-badge-bg-green-soft border-components-badge-status-light-success-border-inner'
      case 'error':
        return 'bg-components-badge-bg-red-soft border-components-badge-status-light-error-border-inner'
      case 'warning':
        return 'bg-components-badge-bg-orange-soft border-components-badge-status-light-warning-border-inner'
      case 'info':
        return 'bg-components-panel-bg border-divider-subtle'
    }
  }

  return (
    <div
      className={cn(
        'flex items-center p-4 mb-3 rounded-lg border shadow-sm transition-all duration-300',
        getBackgroundColor(),
        isVisible ? 'opacity-100 translate-x-0' : 'opacity-0 translate-x-full'
      )}
    >
      {getIcon()}
      <div className="ml-3 text-sm font-medium text-text-primary flex-1">
        {message}
      </div>
      <button
        onClick={handleClose}
        className="ml-3 text-text-tertiary hover:text-text-secondary transition-colors"
      >
        <XMarkIcon className="w-4 h-4" />
      </button>
    </div>
  )
}

// Toast container and manager
class ToastManager {
  private toasts: ToastProps[] = []
  private listeners: Array<(toasts: ToastProps[]) => void> = []

  notify = (options: Omit<ToastProps, 'id' | 'onClose'>) => {
    const id = Math.random().toString(36).substring(2, 11)
    const toast: ToastProps = {
      ...options,
      id,
      onClose: this.remove,
    }
    
    this.toasts.push(toast)
    this.notifyListeners()
  }

  remove = (id: string) => {
    this.toasts = this.toasts.filter(toast => toast.id !== id)
    this.notifyListeners()
  }

  subscribe = (listener: (toasts: ToastProps[]) => void) => {
    this.listeners.push(listener)
    return () => {
      this.listeners = this.listeners.filter(l => l !== listener)
    }
  }

  private notifyListeners = () => {
    this.listeners.forEach(listener => listener([...this.toasts]))
  }
}

export const toastManager = new ToastManager()

export const ToastContainer = () => {
  const [toasts, setToasts] = useState<ToastProps[]>([])
  const [mounted, setMounted] = useState(false)

  useEffect(() => {
    setMounted(true)
    const unsubscribe = toastManager.subscribe(setToasts)
    return unsubscribe
  }, [])

  if (!mounted) return null

  return createPortal(
    <div className="fixed top-4 right-4 z-[9999] max-w-sm w-full">
      {toasts.map(toast => (
        <Toast key={toast.id} {...toast} />
      ))}
    </div>,
    document.body
  )
}

// Export a simple API
const ToastAPI = {
  notify: toastManager.notify,
  success: (message: string, duration?: number) => 
    toastManager.notify({ type: 'success', message, duration }),
  error: (message: string, duration?: number) => 
    toastManager.notify({ type: 'error', message, duration }),
  warning: (message: string, duration?: number) => 
    toastManager.notify({ type: 'warning', message, duration }),
  info: (message: string, duration?: number) => 
    toastManager.notify({ type: 'info', message, duration }),
}

// Context for components that need toast functionality
export const ToastContext = {
  notify: toastManager.notify,
}

// Provider component (for compatibility)
export const ToastProvider = ({ children }: { children: React.ReactNode }) => {
  return <>{children}</>
}

// Hook for using toast context
export const useToastContext = () => ({
  notify: toastManager.notify,
})

export default ToastAPI