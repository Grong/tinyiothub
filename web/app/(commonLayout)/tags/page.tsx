'use client'

import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  RiAddLine,
  RiDeleteBinLine,
  RiEditLine,
  RiSearchLine,
  RiPriceTagLine,
} from '@remixicon/react'
import { useToast } from '@/hooks/use-toast'
import Button from '@/app/components/base/button'
import Input from '@/app/components/base/input'
import Modal from '@/app/components/base/modal'
import Confirm from '@/app/components/base/confirm'
import Loading from '@/app/components/base/loading'
import {
  getAllTags,
  createTag,
  updateTag,
  deleteTag,
  searchTags,
  getTagStats,
  type Tag,
  type CreateTagRequest,
  type TagStats,
} from '@/service/tag'

const TagsPage = () => {
  const { t } = useTranslation('common')
  const { toast } = useToast()
  
  // State
  const [tags, setTags] = useState<Tag[]>([])
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState('')
  const [stats, setStats] = useState<TagStats>({ total: 0, byType: {} })
  
  // Modal states
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [showEditModal, setShowEditModal] = useState(false)
  const [showDeleteModal, setShowDeleteModal] = useState(false)
  const [selectedTag, setSelectedTag] = useState<Tag | null>(null)
  
  // Form states
  const [formData, setFormData] = useState<CreateTagRequest>({
    name: '',
    type: 'device', // 默认类型
    description: '',
    color: '#6B7280',
  })
  const [submitting, setSubmitting] = useState(false)

  // Load data
  const loadTags = async () => {
    try {
      setLoading(true)
      const [tagsData, statsData] = await Promise.all([
        getAllTags(),
        getTagStats(),
      ])
      setTags(tagsData)
      setStats(statsData)
    } catch (error) {
      console.error('Failed to load tags:', error)
      toast.error(t('actionMsg.fetchFailure'))
    } finally {
      setLoading(false)
    }
  }

  // Search tags
  const handleSearch = async (query: string) => {
    if (!query.trim()) {
      loadTags()
      return
    }
    
    try {
      const results = await searchTags(query)
      setTags(results)
    } catch (error) {
      console.error('Failed to search tags:', error)
      toast.error(t('actionMsg.searchFailure'))
    }
  }

  // Create tag
  const handleCreate = async () => {
    if (!formData.name.trim()) {
      toast.error(t('tag.nameRequired'))
      return
    }

    try {
      setSubmitting(true)
      const newTag = await createTag(formData)
      setTags([newTag, ...tags])
      setShowCreateModal(false)
      setFormData({ name: '', type: 'device', description: '', color: '#6B7280' })
      toast.success(t('actionMsg.createdSuccessfully'))
    } catch (error) {
      console.error('Failed to create tag:', error)
      toast.error(t('actionMsg.createdUnsuccessfully'))
    } finally {
      setSubmitting(false)
    }
  }

  // Update tag
  const handleUpdate = async () => {
    if (!selectedTag || !formData.name.trim()) {
      toast.error(t('tag.nameRequired'))
      return
    }

    try {
      setSubmitting(true)
      const updatedTag = await updateTag(selectedTag.id, formData.name)
      setTags(tags.map(tag => tag.id === selectedTag.id ? updatedTag : tag))
      setShowEditModal(false)
      setSelectedTag(null)
      setFormData({ name: '', type: 'device', description: '', color: '#6B7280' })
      toast.success(t('actionMsg.modifiedSuccessfully'))
    } catch (error) {
      console.error('Failed to update tag:', error)
      toast.error(t('actionMsg.modifiedUnsuccessfully'))
    } finally {
      setSubmitting(false)
    }
  }

  // Delete tag
  const handleDelete = async () => {
    if (!selectedTag) return

    try {
      setSubmitting(true)
      await deleteTag(selectedTag.id)
      setTags(tags.filter(tag => tag.id !== selectedTag.id))
      setShowDeleteModal(false)
      setSelectedTag(null)
      toast.success(t('actionMsg.deletedSuccessfully'))
    } catch (error) {
      console.error('Failed to delete tag:', error)
      toast.error(t('actionMsg.deletedUnsuccessfully'))
    } finally {
      setSubmitting(false)
    }
  }

  // Open edit modal
  const openEditModal = (tag: Tag) => {
    setSelectedTag(tag)
    setFormData({
      name: tag.name,
      type: tag.type,
      description: tag.description || '',
      color: tag.color || '#6B7280',
    })
    setShowEditModal(true)
  }

  // Open delete modal
  const openDeleteModal = (tag: Tag) => {
    setSelectedTag(tag)
    setShowDeleteModal(true)
  }

  useEffect(() => {
    loadTags()
  }, [])

  useEffect(() => {
    const timeoutId = setTimeout(() => {
      handleSearch(searchQuery)
    }, 300)
    return () => clearTimeout(timeoutId)
  }, [searchQuery])

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loading />
      </div>
    )
  }

  return (
    <div className="h-full flex flex-col bg-background-body">
      {/* Header */}
      <div className="shrink-0 border-b border-divider-subtle bg-background-default px-6 py-4">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary">
              {t('tag.manageTags')}
            </h1>
            <p className="mt-1 text-sm text-text-tertiary">
              {t('tag.description', { total: stats.total })}
            </p>
          </div>
          <Button
            variant="primary"
            onClick={() => setShowCreateModal(true)}
            className="flex items-center gap-2"
          >
            <RiAddLine className="h-4 w-4" />
            {t('tag.addNew')}
          </Button>
        </div>

        {/* Search */}
        <div className="mt-4 flex items-center gap-4">
          <div className="relative flex-1 max-w-md">
            <RiSearchLine className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-text-tertiary" />
            <Input
              placeholder={t('tag.searchPlaceholder')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-10"
            />
          </div>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-6">
        {tags.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center text-center">
            <RiPriceTagLine className="h-12 w-12 text-text-quaternary" />
            <h3 className="mt-4 text-lg font-medium text-text-secondary">
              {searchQuery ? t('tag.noSearchResults') : t('tag.noTags')}
            </h3>
            <p className="mt-2 text-sm text-text-tertiary">
              {searchQuery 
                ? t('tag.tryDifferentSearch')
                : t('tag.createFirstTag')
              }
            </p>
            {!searchQuery && (
              <Button
                variant="primary"
                onClick={() => setShowCreateModal(true)}
                className="mt-4 flex items-center gap-2"
              >
                <RiAddLine className="h-4 w-4" />
                {t('tag.addNew')}
              </Button>
            )}
          </div>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            {tags.map((tag) => (
              <div
                key={tag.id}
                className="rounded-lg border border-divider-subtle bg-background-default p-4 hover:border-divider-regular transition-colors"
              >
                <div className="flex items-start justify-between">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <div
                        className="h-3 w-3 rounded-full"
                        style={{ backgroundColor: tag.color || '#6B7280' }}
                      />
                      <h3 className="font-medium text-text-primary truncate">
                        {tag.name}
                      </h3>
                    </div>
                    {tag.description && (
                      <p className="mt-1 text-sm text-text-tertiary line-clamp-2">
                        {tag.description}
                      </p>
                    )}
                    <div className="mt-2 flex items-center gap-4 text-xs text-text-quaternary">
                      <span>{t('tag.usageCount', { count: tag.bindingCount || 0 })}</span>
                      <span>{new Date(tag.createdAt).toLocaleDateString()}</span>
                    </div>
                  </div>
                  <div className="flex items-center gap-1 ml-2">
                    <button
                      onClick={() => openEditModal(tag)}
                      className="p-1 rounded hover:bg-background-default-hover text-text-tertiary hover:text-text-secondary"
                    >
                      <RiEditLine className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => openDeleteModal(tag)}
                      className="p-1 rounded hover:bg-background-default-hover text-text-tertiary hover:text-text-destructive"
                    >
                      <RiDeleteBinLine className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Create Modal */}
      <Modal
        isShow={showCreateModal}
        onClose={() => setShowCreateModal(false)}
        title={t('tag.createTag')}
      >
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-2">
              {t('tag.name')}
            </label>
            <Input
              placeholder={t('tag.namePlaceholder')}
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-2">
              {t('tag.description')}
            </label>
            <Input
              placeholder={t('tag.descriptionPlaceholder')}
              value={formData.description}
              onChange={(e) => setFormData({ ...formData, description: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-2">
              {t('tag.color')}
            </label>
            <div className="flex items-center gap-2">
              <input
                type="color"
                value={formData.color}
                onChange={(e) => setFormData({ ...formData, color: e.target.value })}
                className="h-10 w-16 rounded border border-divider-subtle"
              />
              <Input
                value={formData.color}
                onChange={(e) => setFormData({ ...formData, color: e.target.value })}
                placeholder={t('tag.colorPlaceholder')}
                className="flex-1"
              />
            </div>
          </div>
          <div className="flex justify-end gap-2 pt-4">
            <Button
              variant="secondary"
              onClick={() => setShowCreateModal(false)}
              disabled={submitting}
            >
              {t('operation.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={handleCreate}
              loading={submitting}
            >
              {t('operation.create')}
            </Button>
          </div>
        </div>
      </Modal>

      {/* Edit Modal */}
      <Modal
        isShow={showEditModal}
        onClose={() => setShowEditModal(false)}
        title={t('tag.editTag')}
      >
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-text-secondary mb-2">
              {t('tag.name')}
            </label>
            <Input
              placeholder={t('tag.namePlaceholder')}
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
            />
          </div>
          <div className="flex justify-end gap-2 pt-4">
            <Button
              variant="secondary"
              onClick={() => setShowEditModal(false)}
              disabled={submitting}
            >
              {t('operation.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={handleUpdate}
              loading={submitting}
            >
              {t('operation.save')}
            </Button>
          </div>
        </div>
      </Modal>

      {/* Delete Confirmation */}
      <Confirm
        isShow={showDeleteModal}
        onCancel={() => setShowDeleteModal(false)}
        onConfirm={handleDelete}
        title={t('tag.deleteTag')}
        content={t('tag.deleteConfirm', { name: selectedTag?.name })}
        confirmText={t('operation.delete')}
        isLoading={submitting}
      />
    </div>
  )
}

export default TagsPage