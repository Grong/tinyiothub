import { describe, it, expect } from 'vitest'
import { toCamelCase, toSnakeCase, keysToCamelCase, keysToSnakeCase } from './case-converter'

describe('toCamelCase', () => {
  it('converts snake_case to camelCase', () => {
    expect(toCamelCase('created_at')).toBe('createdAt')
    expect(toCamelCase('device_id')).toBe('deviceId')
    expect(toCamelCase('trace_type')).toBe('traceType')
  })

  it('handles multiple underscores', () => {
    expect(toCamelCase('last_event_time')).toBe('lastEventTime')
    expect(toCamelCase('total_traces_count')).toBe('totalTracesCount')
  })

  it('leaves already camelCase unchanged', () => {
    expect(toCamelCase('createdAt')).toBe('createdAt')
    expect(toCamelCase('deviceId')).toBe('deviceId')
  })

  it('leaves strings without underscores unchanged', () => {
    expect(toCamelCase('name')).toBe('name')
    expect(toCamelCase('status')).toBe('status')
  })
})

describe('toSnakeCase', () => {
  it('converts camelCase to snake_case', () => {
    expect(toSnakeCase('createdAt')).toBe('created_at')
    expect(toSnakeCase('deviceId')).toBe('device_id')
    expect(toSnakeCase('traceType')).toBe('trace_type')
  })

  it('handles multiple capital letters', () => {
    expect(toSnakeCase('lastEventTime')).toBe('last_event_time')
    expect(toSnakeCase('totalTracesCount')).toBe('total_traces_count')
  })

  it('leaves already snake_case unchanged', () => {
    expect(toSnakeCase('created_at')).toBe('created_at')
    expect(toSnakeCase('device_id')).toBe('device_id')
  })
})

describe('keysToCamelCase', () => {
  it('converts all keys in a simple object', () => {
    const input = { created_at: '2024-01-01', device_id: 'dev-1' }
    const expected = { createdAt: '2024-01-01', deviceId: 'dev-1' }
    expect(keysToCamelCase(input)).toEqual(expected)
  })

  it('converts DeviceTrace from API (snake_case) to camelCase', () => {
    // This is the exact bug scenario: API returns snake_case, we need camelCase
    const apiResponse = {
      id: '1',
      device_id: 'dev-1',
      trace_type: 'info',
      level: 'info',
      category: 'system',
      title: 'Test trace',
      message: 'Test message',
      created_at: '2024-01-01T00:00:00Z',
    }

    const converted = keysToCamelCase(apiResponse)

    expect(converted).toEqual({
      id: '1',
      deviceId: 'dev-1',
      traceType: 'info',
      level: 'info',
      category: 'system',
      title: 'Test trace',
      message: 'Test message',
      createdAt: '2024-01-01T00:00:00Z',
    })
    // These should NOT exist
    expect((converted as any).device_id).toBeUndefined()
    expect((converted as any).created_at).toBeUndefined()
  })

  it('handles nested objects', () => {
    const input = {
      device_info: {
        device_id: 'dev-1',
        created_at: '2024-01-01',
      },
    }

    const converted = keysToCamelCase(input)

    expect(converted).toEqual({
      deviceInfo: {
        deviceId: 'dev-1',
        createdAt: '2024-01-01',
      },
    })
  })

  it('handles arrays of objects', () => {
    const input = [
      { device_id: 'dev-1', created_at: '2024-01-01' },
      { device_id: 'dev-2', created_at: '2024-01-02' },
    ]

    const converted = keysToCamelCase(input)

    expect(converted).toEqual([
      { deviceId: 'dev-1', createdAt: '2024-01-01' },
      { deviceId: 'dev-2', createdAt: '2024-01-02' },
    ])
  })

  it('handles null and undefined', () => {
    expect(keysToCamelCase(null)).toBeNull()
    expect(keysToCamelCase(undefined)).toBeUndefined()
  })

  it('handles primitives unchanged', () => {
    expect(keysToCamelCase('string')).toBe('string')
    expect(keysToCamelCase(123)).toBe(123)
    expect(keysToCamelCase(true)).toBe(true)
  })

  it('handles arrays of primitives unchanged', () => {
    expect(keysToCamelCase([1, 2, 3])).toEqual([1, 2, 3])
    expect(keysToCamelCase(['a', 'b'])).toEqual(['a', 'b'])
  })
})

describe('keysToSnakeCase', () => {
  it('converts camelCase keys to snake_case', () => {
    const input = { createdAt: '2024-01-01', deviceId: 'dev-1' }
    const expected = { created_at: '2024-01-01', device_id: 'dev-1' }
    expect(keysToSnakeCase(input)).toEqual(expected)
  })

  it('handles nested objects', () => {
    const input = {
      deviceInfo: {
        deviceId: 'dev-1',
        createdAt: '2024-01-01',
      },
    }

    const converted = keysToSnakeCase(input)

    expect(converted).toEqual({
      device_info: {
        device_id: 'dev-1',
        created_at: '2024-01-01',
      },
    })
  })

  it('handles arrays of objects', () => {
    const input = [
      { deviceId: 'dev-1', createdAt: '2024-01-01' },
      { deviceId: 'dev-2', createdAt: '2024-01-02' },
    ]

    const converted = keysToSnakeCase(input)

    expect(converted).toEqual([
      { device_id: 'dev-1', created_at: '2024-01-01' },
      { device_id: 'dev-2', created_at: '2024-01-02' },
    ])
  })
})
