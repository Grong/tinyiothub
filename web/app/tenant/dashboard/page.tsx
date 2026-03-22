'use client'

import { useState, useEffect } from 'react'
import { useRouter } from 'next/navigation'
import { tenantApi, getStoredTenant, clearTenantData, type Tenant, type ApiKey } from '@/service/tenant'

export default function DashboardPage() {
  const router = useRouter()
  const [tenant, setTenant] = useState<Tenant | null>(null)
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [showKeyModal, setShowKeyModal] = useState(false)
  const [newKeyName, setNewKeyName] = useState('')
  const [newKeyPermissions, setNewKeyPermissions] = useState<string[]>(['read'])
  const [createdKey, setCreatedKey] = useState('')

  useEffect(() => {
    const storedTenant = getStoredTenant()

    if (!storedTenant) {
      router.push('/tenant/login')
      return
    }

    setTenant(storedTenant)
    loadApiKeys(storedTenant.id)
  }, [router])

  const loadApiKeys = async (tenantId: string) => {
    try {
      const response = await tenantApi.getApiKeys(tenantId)
      if (response.code === 0) {
        setApiKeys(response.result || [])
      }
    } catch (err) {
      console.error('Failed to load API keys:', err)
    } finally {
      setIsLoading(false)
    }
  }

  const handleCreateKey = async () => {
    if (!tenant || !newKeyName) return

    try {
      const response = await tenantApi.createApiKey(tenant.id, {
        name: newKeyName,
        permissions: newKeyPermissions,
      })

      if (response.code === 0 && response.result) {
        setCreatedKey(response.result.raw_key)
        loadApiKeys(tenant.id)
      }
    } catch (err) {
      console.error('Failed to create API key:', err)
    }
  }

  const handleLogout = () => {
    clearTenantData()
    router.push('/tenant/login')
  }

  if (isLoading || !tenant) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary-600"></div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-100">
      {/* 顶部导航 */}
      <nav className="bg-white shadow-sm">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex justify-between h-16">
            <div className="flex items-center">
              <h1 className="text-xl font-bold text-gray-900">TinyIoTHub</h1>
              <span className="ml-2 text-sm text-gray-500">/ {tenant.name}</span>
            </div>
            <div className="flex items-center">
              <button
                onClick={handleLogout}
                className="ml-4 px-4 py-2 text-sm text-gray-700 hover:text-gray-900"
              >
                退出登录
              </button>
            </div>
          </div>
        </div>
      </nav>

      <main className="max-w-7xl mx-auto py-6 sm:px-6 lg:px-8">
        {/* 租户信息卡片 */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6">
          <div className="bg-white rounded-lg shadow p-6">
            <h3 className="text-sm font-medium text-gray-500">组织名称</h3>
            <p className="mt-2 text-2xl font-semibold text-gray-900">{tenant.name}</p>
          </div>
          <div className="bg-white rounded-lg shadow p-6">
            <h3 className="text-sm font-medium text-gray-500">订阅计划</h3>
            <p className="mt-2 text-2xl font-semibold text-gray-900">{tenant.plan_id}</p>
          </div>
          <div className="bg-white rounded-lg shadow p-6">
            <h3 className="text-sm font-medium text-gray-500">状态</h3>
            <p className="mt-2 text-2xl font-semibold text-gray-900">
              <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${
                tenant.status === 'active' ? 'bg-green-100 text-green-800' : 'bg-yellow-100 text-yellow-800'
              }`}>
                {tenant.status === 'active' ? '活跃' : '试用中'}
              </span>
            </p>
          </div>
        </div>

        {/* API Keys 管理 */}
        <div className="bg-white rounded-lg shadow">
          <div className="px-6 py-4 border-b border-gray-200 flex justify-between items-center">
            <h2 className="text-lg font-medium text-gray-900">API Keys</h2>
            <button
              onClick={() => setShowKeyModal(true)}
              className="px-4 py-2 bg-primary-600 text-white text-sm rounded-lg hover:bg-primary-700"
            >
              创建 API Key
            </button>
          </div>
          
          <div className="p-6">
            {apiKeys.length === 0 ? (
              <p className="text-gray-500 text-center py-8">暂无 API Keys</p>
            ) : (
              <div className="space-y-4">
                {apiKeys.map((key) => (
                  <div key={key.id} className="flex items-center justify-between p-4 border border-gray-200 rounded-lg">
                    <div>
                      <p className="font-medium text-gray-900">{key.name}</p>
                      <p className="text-sm text-gray-500 mt-1">{key.prefix}****</p>
                      <p className="text-xs text-gray-400 mt-1">
                        创建于 {new Date(key.created_at).toLocaleDateString()}
                      </p>
                    </div>
                    <div className="flex items-center gap-4">
                      <span className={`text-xs px-2 py-1 rounded ${
                        key.is_enabled ? 'bg-green-100 text-green-800' : 'bg-gray-100 text-gray-800'
                      }`}>
                        {key.is_enabled ? '启用' : '禁用'}
                      </span>
                      <span className="text-sm text-gray-500">
                        {key.request_count} 次调用
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* 开放 API 说明 */}
        <div className="mt-6 bg-white rounded-lg shadow p-6">
          <h2 className="text-lg font-medium text-gray-900 mb-4">使用文档</h2>
          <div className="prose prose-sm max-w-none">
            <h3 className="font-medium text-gray-900">调用示例</h3>
            <pre className="bg-gray-50 p-4 rounded-lg text-sm overflow-x-auto">
{`curl -X GET "https://api.tinyiothub.com/open/devices" \\
  -H "X-API-Key: your-api-key-here"`}
            </pre>
            
            <h3 className="font-medium text-gray-900 mt-4">可用接口</h3>
            <ul className="list-disc pl-4 space-y-1 text-sm text-gray-600">
              <li>GET /open/devices - 设备列表</li>
              <li>GET /open/devices/:id - 设备详情</li>
              <li>GET /open/devices/:id/properties - 设备属性</li>
              <li>POST /open/devices/:id/command - 发送命令</li>
              <li>GET /open/events - 事件列表</li>
            </ul>
          </div>
        </div>
      </main>

      {/* 创建 API Key 弹窗 */}
      {showKeyModal && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-lg p-6 w-full max-w-md">
            <h3 className="text-lg font-medium text-gray-900 mb-4">创建 API Key</h3>
            
            {createdKey ? (
              <div>
                <div className="mb-4 p-3 bg-green-50 border border-green-200 rounded-lg">
                  <p className="text-sm text-green-800 font-medium">API Key 创建成功！</p>
                  <p className="text-xs text-green-600 mt-1">请妥善保存，此密钥只会显示一次</p>
                </div>
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 mb-1">API Key</label>
                  <code className="block w-full p-2 bg-gray-50 border border-gray-200 rounded text-sm break-all">
                    {createdKey}
                  </code>
                </div>
                <button
                  onClick={() => { setShowKeyModal(false); setCreatedKey(''); }}
                  className="w-full py-2 bg-primary-600 text-white rounded-lg hover:bg-primary-700"
                >
                  完成
                </button>
              </div>
            ) : (
              <form onSubmit={(e) => { e.preventDefault(); handleCreateKey(); }}>
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 mb-1">名称</label>
                  <input
                    type="text"
                    value={newKeyName}
                    onChange={(e) => setNewKeyName(e.target.value)}
                    required
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg"
                    placeholder="生产环境 Key"
                  />
                </div>
                <div className="mb-4">
                  <label className="block text-sm font-medium text-gray-700 mb-1">权限</label>
                  <select
                    multiple
                    value={newKeyPermissions}
                    onChange={(e) => setNewKeyPermissions(Array.from(e.target.selectedOptions, o => o.value))}
                    className="w-full px-3 py-2 border border-gray-300 rounded-lg"
                  >
                    <option value="read">读取</option>
                    <option value="write">写入</option>
                  </select>
                </div>
                <div className="flex gap-3">
                  <button
                    type="button"
                    onClick={() => setShowKeyModal(false)}
                    className="flex-1 py-2 border border-gray-300 text-gray-700 rounded-lg hover:bg-gray-50"
                  >
                    取消
                  </button>
                  <button
                    type="submit"
                    className="flex-1 py-2 bg-primary-600 text-white rounded-lg hover:bg-primary-700"
                  >
                    创建
                  </button>
                </div>
              </form>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
