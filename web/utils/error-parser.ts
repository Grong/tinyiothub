export interface ApiError {
  code?: string
  message: string
  details?: any
}

export const parseApiError = (error: any): ApiError => {
  if (typeof error === 'string') {
    return { message: error }
  }

  if (error?.response?.data) {
    const { code, message, detail } = error.response.data
    return {
      code,
      message: message || 'Unknown error occurred',
      details: detail
    }
  }

  if (error?.message) {
    return { message: error.message }
  }

  return { message: 'Unknown error occurred' }
}

export const getErrorMessage = (error: any): string => {
  const parsedError = parseApiError(error)
  return parsedError.message
}