'use client'
import React, { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { RiAddLine, RiDeleteBinLine, RiEditLine, RiMore2Fill, RiUserLine } from '@remixicon/react'
import { Menu, MenuButton, MenuItem, MenuItems } from '@headlessui/react'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'
import Avatar from '@/app/components/base/avatar'
import { useAuthGuard } from '@/hooks/use-auth-guard'
import { 
  useUsers, 
  useCreateUser, 
  useUpdateUser, 
  useDeleteUser, 
  useToggleUserStatus,
} from '@/service/users'
import type { User, CreateUserRequest, UpdateUserRequest } from '@/types'
import cn from '@/utils/classnames'

interface FormData {
  name: string
  password: string
  email: string
  phone: string
}

const MembersPage = () => {
  const { t } = useTranslation('common')
  
  // Authentication guard - redirect to login if not authenticated
  const { shouldRender } = useAuthGuard()
  
  const [showAddModal, setShowAddModal] = useState(false)
  const [editingUser, setEditingUser] = useState<User | null>(null)
  const [formData, setFormData] = useState<FormData>({
    name: '',
    password: '',
    email: '',
    phone: '',
  })

  // 使用 TanStack Query hooks
  const { 
    data: usersResponse, 
    isLoading, 
    error,
    refetch 
  } = useUsers({
    page: 1,
    page_size: 50  // 使用 snake_case
  })

  const createUserMutation = useCreateUser()
  const updateUserMutation = useUpdateUser()
  const deleteUserMutation = useDeleteUser()
  const toggleUserStatusMutation = useToggleUserStatus()

  // Don't render if not authenticated (will redirect to login)
  if (!shouldRender) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="text-center">
          <div className="system-md-regular text-text-tertiary">Checking authentication...</div>
        </div>
      </div>
    )
  }

  const users = usersResponse?.result || []

  // 创建用户
  const handleCreateUser = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!formData.name.trim() || !formData.password.trim()) {
      return
    }

    try {
      const createData: CreateUserRequest = {
        name: formData.name,
        username: formData.name, // 使用name作为username
        password: formData.password,
        email: formData.email || undefined,
        phone: formData.phone || undefined,
      }
      
      await createUserMutation.mutateAsync(createData)
      setShowAddModal(false)
      setFormData({ name: '', password: '', email: '', phone: '' })
    } catch (error) {
      console.error('Failed to create user:', error)
      alert('Failed to create user: ' + (error as Error).message)
    }
  }

  // 更新用户
  const handleUpdateUser = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!editingUser || !formData.name.trim()) {
      return
    }

    try {
      const updateData: UpdateUserRequest = {
        name: formData.name,
        email: formData.email || undefined,
        phone: formData.phone || undefined,
      }
      
      await updateUserMutation.mutateAsync({
        id: editingUser.id,
        data: updateData
      })
      
      setEditingUser(null)
      setFormData({ name: '', password: '', email: '', phone: '' })
    } catch (error) {
      console.error('Failed to update user:', error)
      alert('Failed to update user: ' + (error as Error).message)
    }
  }

  // 删除用户
  const handleDeleteUser = async (userId: string) => {
    if (!confirm('Are you sure you want to delete this user?')) {
      return
    }

    try {
      await deleteUserMutation.mutateAsync(userId)
    } catch (error) {
      console.error('Failed to delete user:', error)
      alert('Failed to delete user: ' + (error as Error).message)
    }
  }

  // 启用/禁用用户
  const handleToggleUserStatus = async (user: User) => {
    const enabled = !user.isDisabled // 如果当前是禁用状态，则启用
    
    try {
      await toggleUserStatusMutation.mutateAsync({
        id: user.id,
        enabled
      })
    } catch (error) {
      console.error('Failed to toggle user status:', error)
      alert('Failed to toggle user status: ' + (error as Error).message)
    }
  }

  const openEditModal = (user: User) => {
    setEditingUser(user)
    setFormData({
      name: user.name,
      password: '', // 不显示现有密码
      email: user.email || '',
      phone: user.phone || '',
    })
  }

  const closeModal = () => {
    setShowAddModal(false)
    setEditingUser(null)
    setFormData({ name: '', password: '', email: '', phone: '' })
  }

  // 错误处理
  if (error) {
    const errorMessage = error instanceof Error ? error.message : 'Something went wrong'
    const isAuthError = errorMessage.includes('Unauthorized') || errorMessage.includes('401')
    
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <div className="text-text-secondary mb-4">
          {isAuthError ? 'Authentication required' : 'Something went wrong'}
        </div>
        <div className="text-text-tertiary mb-4 text-sm">
          {isAuthError ? 'Please log in to access this page' : errorMessage}
        </div>
        <Button onClick={() => refetch()}>
          {isAuthError ? 'Go to Login' : 'Retry'}
        </Button>
      </div>
    )
  }

  // 加载状态
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="text-center">
          <div className="system-md-regular text-text-tertiary">Loading...</div>
        </div>
      </div>
    )
  }

  return (
    <div className="max-w-4xl">
      {/* 头部操作区 */}
      <div className="mb-6 flex items-center justify-between">
        <div>
          <div className="system-lg-semibold text-text-primary mb-1">Members</div>
          <div className="system-sm-regular text-text-tertiary">
            Manage team members and their permissions
          </div>
        </div>
        <Button
          variant="primary"
          size="medium"
          onClick={() => setShowAddModal(true)}
          className="flex items-center gap-2"
          disabled={createUserMutation.isPending}
        >
          <RiAddLine className="w-4 h-4" />
          Add Member
        </Button>
      </div>

      {/* 用户列表 */}
      <div className="bg-components-panel-bg border border-divider-subtle rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-divider-subtle bg-components-panel-bg-alt">
          <div className="grid grid-cols-12 gap-4 system-sm-medium text-text-secondary">
            <div className="col-span-4">User</div>
            <div className="col-span-3">Contact</div>
            <div className="col-span-2">Status</div>
            <div className="col-span-2">Last Login</div>
            <div className="col-span-1">Actions</div>
          </div>
        </div>
        
        <div className="divide-y divide-divider-subtle">
          {users.map((user) => (
            <div key={user.id} className="px-6 py-4 hover:bg-state-base-hover transition-colors">
              <div className="grid grid-cols-12 gap-4 items-center">
                {/* 用户信息 */}
                <div className="col-span-4 flex items-center space-x-3">
                  <Avatar name={user.name} size={40} />
                  <div>
                    <div className="system-sm-semibold text-text-primary">{user.name}</div>
                    {user.email && (
                      <div className="system-xs-regular text-text-tertiary">{user.email}</div>
                    )}
                  </div>
                </div>

                {/* 联系方式 */}
                <div className="col-span-3">
                  <div className="system-sm-regular text-text-secondary">
                    {user.phone || user.email || '-'}
                  </div>
                </div>

                {/* 状态 */}
                <div className="col-span-2">
                  <span className={cn(
                    'inline-flex items-center px-2 py-1 rounded-full text-xs font-medium',
                    !user.isDisabled
                      ? 'bg-green-100 text-green-800'
                      : 'bg-red-100 text-red-800'
                  )}>
                    {!user.isDisabled ? 'Active' : 'Disabled'}
                  </span>
                </div>

                {/* 最后登录 */}
                <div className="col-span-2">
                  <div className="system-xs-regular text-text-tertiary">
                    {user.dateLastLogon ? new Date(user.dateLastLogon).toLocaleDateString() : 'Never'}
                  </div>
                </div>

                {/* 操作菜单 */}
                <div className="col-span-1 flex justify-end">
                  <Menu as="div" className="relative">
                    <MenuButton className="p-1 hover:bg-state-base-hover rounded-md transition-colors">
                      <RiMore2Fill className="w-4 h-4 text-text-tertiary" />
                    </MenuButton>
                    <MenuItems className="absolute right-0 mt-1 w-48 bg-components-panel-bg border border-divider-subtle rounded-lg shadow-lg z-10">
                      <MenuItem>
                        <button
                          className="flex items-center w-full px-3 py-2 text-sm text-text-primary hover:bg-state-base-hover"
                          onClick={() => openEditModal(user)}
                          disabled={updateUserMutation.isPending}
                        >
                          <RiEditLine className="w-4 h-4 mr-2" />
                          Edit
                        </button>
                      </MenuItem>
                      <MenuItem>
                        <button
                          className="flex items-center w-full px-3 py-2 text-sm text-text-primary hover:bg-state-base-hover"
                          onClick={() => handleToggleUserStatus(user)}
                          disabled={toggleUserStatusMutation.isPending}
                        >
                          <RiUserLine className="w-4 h-4 mr-2" />
                          {!user.isDisabled ? 'Disable' : 'Enable'}
                        </button>
                      </MenuItem>
                      <MenuItem>
                        <button
                          className="flex items-center w-full px-3 py-2 text-sm text-red-600 hover:bg-red-50"
                          onClick={() => handleDeleteUser(user.id)}
                          disabled={deleteUserMutation.isPending}
                        >
                          <RiDeleteBinLine className="w-4 h-4 mr-2" />
                          Delete
                        </button>
                      </MenuItem>
                    </MenuItems>
                  </Menu>
                </div>
              </div>
            </div>
          ))}
        </div>

        {users.length === 0 && (
          <div className="px-6 py-12 text-center">
            <RiUserLine className="w-12 h-12 text-text-quaternary mx-auto mb-4" />
            <div className="system-md-regular text-text-tertiary mb-2">No members found</div>
            <div className="system-sm-regular text-text-quaternary">Click "Add Member" to get started</div>
          </div>
        )}
      </div>

      {/* 添加/编辑用户模态框 */}
      {(showAddModal || editingUser) && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div className="bg-white rounded-xl p-6 w-full max-w-md mx-4">
            <h3 className="text-lg font-semibold text-gray-900 mb-4">
              {editingUser ? 'Edit Member' : 'Add Member'}
            </h3>
            
            <form onSubmit={editingUser ? handleUpdateUser : handleCreateUser} className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Name *
                </label>
                <Input
                  type="text"
                  value={formData.name}
                  onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                  placeholder="Enter user name"
                  required
                />
              </div>

              {!editingUser && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Password *
                  </label>
                  <Input
                    type="password"
                    value={formData.password}
                    onChange={(e) => setFormData({ ...formData, password: e.target.value })}
                    placeholder="Enter password"
                    required
                  />
                </div>
              )}

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Email
                </label>
                <Input
                  type="email"
                  value={formData.email}
                  onChange={(e) => setFormData({ ...formData, email: e.target.value })}
                  placeholder="Enter email address"
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Phone
                </label>
                <Input
                  type="tel"
                  value={formData.phone}
                  onChange={(e) => setFormData({ ...formData, phone: e.target.value })}
                  placeholder="Enter phone number"
                />
              </div>

              <div className="flex justify-end space-x-3 pt-4">
                <Button
                  type="button"
                  variant="secondary"
                  onClick={closeModal}
                  disabled={createUserMutation.isPending || updateUserMutation.isPending}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  variant="primary"
                  disabled={createUserMutation.isPending || updateUserMutation.isPending}
                >
                  {editingUser ? 'Save' : 'Add'}
                </Button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}

export default MembersPage