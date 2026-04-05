import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { DeviceTemplate, DeviceInfo } from './templates'

// Mock the api-client module
vi.mock('../lib/api-client', () => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
}))

import { apiGet, apiPost } from '../lib/api-client'
import { templateApi, transformDeviceTemplate, isFieldRequired } from './templates'

describe('transformDeviceTemplate', () => {
  it('converts raw API template to processed template', () => {
    const raw: DeviceTemplate = {
      id: 'tpl-1',
      name: 'modbus-sensor',
      displayName: '{"zh":"Modbus 传感器","en":"Modbus Sensor"}',
      description: '{"zh":"描述","en":"Description"}',
      version: '1.0.0',
      author: 'Admin',
      category: 'industrial',
      manufacturer: 'Acme',
      deviceType: 'sensor',
      protocolType: 'modbus-tcp',
      driverName: 'modbus-tcp',
      tags: '["temperature","industrial"]',
      deviceInfo: '{"requiredFields":["name","address"],"defaultNamePattern":"{name}"}',
      properties: '[{"name":"temp","dataType":"float"}]',
      commands: '[{"name":"restart","isRequired":false}]',
      isBuiltin: 1,
      isActive: 1,
      createdAt: '2024-01-01T00:00:00Z',
      updatedAt: '2024-01-02T00:00:00Z',
    }

    const result = transformDeviceTemplate(raw)

    expect(result.id).toBe('tpl-1')
    expect(result.name).toBe('modbus-sensor')
    expect(result.displayName).toEqual({ zh: 'Modbus 传感器', en: 'Modbus Sensor' })
    expect(result.description).toEqual({ zh: '描述', en: 'Description' })
    expect(result.tags).toEqual(['temperature', 'industrial'])
    expect(result.deviceInfo.requiredFields).toEqual(['name', 'address'])
    expect(result.properties).toEqual([{ name: 'temp', dataType: 'float' }])
    expect(result.commands).toEqual([{ name: 'restart', isRequired: false }])
    expect(result.isBuiltin).toBe(true)
    expect(result.isActive).toBe(true)
  })

  it('handles null/malformed JSON gracefully with fallbacks', () => {
    const raw: DeviceTemplate = {
      id: 'tpl-2',
      name: 'test',
      displayName: '',
      description: null,
      version: '1.0',
      category: 'test',
      deviceType: 'generic',
      tags: '',
      deviceInfo: 'not json',
      properties: 'also not json',
      commands: '',
      isBuiltin: 0,
      isActive: 0,
      createdAt: '',
      updatedAt: '',
    }

    const result = transformDeviceTemplate(raw)

    expect(result.displayName).toEqual({})
    expect(result.description).toBeNull()
    expect(result.tags).toEqual([])
    expect(result.deviceInfo).toEqual({})
    expect(result.properties).toEqual([])
    expect(result.commands).toEqual([])
    expect(result.isBuiltin).toBe(false)
    expect(result.isActive).toBe(false)
  })

  it('converts isBuiltin/isActive from 0/1 to boolean', () => {
    const builtin: DeviceTemplate = { ...{} as DeviceTemplate, isBuiltin: 1, isActive: 1, name: 't', displayName: '{}', description: null, version: '1', category: '', deviceType: '', tags: '', deviceInfo: '', properties: '', commands: '', createdAt: '', updatedAt: '' }
    const notBuiltin: DeviceTemplate = { ...builtin, isBuiltin: 0, isActive: 0 }

    expect(transformDeviceTemplate(builtin).isBuiltin).toBe(true)
    expect(transformDeviceTemplate(builtin).isActive).toBe(true)
    expect(transformDeviceTemplate(notBuiltin).isBuiltin).toBe(false)
    expect(transformDeviceTemplate(notBuiltin).isActive).toBe(false)
  })
})

describe('isFieldRequired', () => {
  it('returns true if field is in requiredFields', () => {
    const deviceInfo: DeviceInfo = {
      defaultNamePattern: '{name}',
      requiredFields: ['name', 'address'],
    }
    expect(isFieldRequired(deviceInfo, 'name')).toBe(true)
    expect(isFieldRequired(deviceInfo, 'address')).toBe(true)
  })

  it('returns false if field is not in requiredFields', () => {
    const deviceInfo: DeviceInfo = {
      defaultNamePattern: '{name}',
      requiredFields: ['name', 'address'],
    }
    expect(isFieldRequired(deviceInfo, 'description')).toBe(false)
  })

  it('returns false if deviceInfo is undefined', () => {
    expect(isFieldRequired(undefined, 'name')).toBe(false)
  })

  it('returns false if requiredFields is empty', () => {
    const deviceInfo: DeviceInfo = {
      defaultNamePattern: '{name}',
      requiredFields: [],
    }
    expect(isFieldRequired(deviceInfo, 'name')).toBe(false)
  })
})

describe('templateApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('getTemplates', () => {
    it('calls apiGet with correct endpoint and params', async () => {
      const mockResponse = { code: 0, msg: '', result: [] }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await templateApi.getTemplates({ category: 'industrial', page: 1 })

      expect(apiGet).toHaveBeenCalledWith('device-templates', { category: 'industrial', page: 1 })
      expect(result.result).toEqual([])
    })
  })

  describe('getTemplate', () => {
    it('calls apiGet with template id', async () => {
      const mockResponse = { code: 0, msg: '', result: { id: 'tpl-1', name: 'test' } }
      ;(apiGet as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const result = await templateApi.getTemplate('tpl-1')

      expect(apiGet).toHaveBeenCalledWith('device-templates/tpl-1')
      expect(result.result?.id).toBe('tpl-1')
    })
  })

  describe('validateTemplate', () => {
    it('calls apiPost with template id and input', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: { isValid: true, errors: [], warnings: [] },
      }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const input = { name: 'my-device', propertyValues: {}, enabledCommands: [] }
      const result = await templateApi.validateTemplate('tpl-1', input)

      expect(apiPost).toHaveBeenCalledWith('device-templates/tpl-1/validate', input)
      expect(result.result?.isValid).toBe(true)
    })
  })

  describe('previewDevice', () => {
    it('calls apiPost with template id and input', async () => {
      const mockResponse = {
        code: 0,
        msg: '',
        result: { deviceInfo: {}, properties: [], commands: [], warnings: [] },
      }
      ;(apiPost as ReturnType<typeof vi.fn>).mockResolvedValue(mockResponse)

      const input = { name: 'my-device', propertyValues: {}, enabledCommands: [] }
      const result = await templateApi.previewDevice('tpl-1', input)

      expect(apiPost).toHaveBeenCalledWith('device-templates/tpl-1/preview', input)
      expect(result.result?.warnings).toEqual([])
    })
  })
})
