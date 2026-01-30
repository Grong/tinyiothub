import dayjs from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import 'dayjs/locale/zh-cn'

dayjs.extend(relativeTime)
dayjs.locale('zh-cn')

// 格式化日期时间
export const formatDateTime = (date: string | Date, format = 'YYYY-MM-DD HH:mm:ss') => {
  return dayjs(date).format(format)
}

// 格式化相对时间
export const formatRelativeTime = (date: string | Date) => {
  return dayjs(date).fromNow()
}

// 格式化文件大小
export const formatFileSize = (bytes: number) => {
  if (bytes === 0) return '0 B'
  
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

// 格式化数字
export const formatNumber = (num: number, precision = 2) => {
  if (num >= 1000000) {
    return (num / 1000000).toFixed(precision) + 'M'
  }
  if (num >= 1000) {
    return (num / 1000).toFixed(precision) + 'K'
  }
  return num.toString()
}

// 格式化百分比
export const formatPercentage = (value: number, total: number, precision = 1) => {
  if (total === 0) return '0%'
  return ((value / total) * 100).toFixed(precision) + '%'
}

// 格式化设备状态
export const formatDeviceStatus = (status: string) => {
  const statusMap = {
    online: '在线',
    offline: '离线',
    error: '异常',
    connecting: '连接中',
  }
  return statusMap[status as keyof typeof statusMap] || status
}

// 格式化告警级别
export const formatAlarmLevel = (level: string) => {
  const levelMap = {
    low: '低',
    medium: '中',
    high: '高',
    critical: '严重',
  }
  return levelMap[level as keyof typeof levelMap] || level
}