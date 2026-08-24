use std::sync::Arc;

use tinyiothub_core::error::{Error, Result};

use tinyiothub_core::models::tag::{CreateTagBindingRequest, CreateTagRequest, UpdateTagRequest};
use tinyiothub_storage::Db;
use tinyiothub_storage::tag::{Tag, TagBinding, TagQuery};

pub struct TagService {
    db: Arc<Db>,
}

impl TagService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn find_tag_by_id(&self, id: &str) -> Result<Option<Tag>> {
        self.db.find_tag_by_id(id).await
    }

    pub async fn find_tag_by_name_and_type(&self, name: &str, tag_type: &str) -> Result<Option<Tag>> {
        self.db.find_tag_by_name_and_type(name, tag_type).await
    }

    pub async fn create_tag(&self, request: &CreateTagRequest, created_by: &str, tenant_id: &str) -> Result<Tag> {
        self.db.create_tag(request, created_by, tenant_id).await
    }

    pub async fn update_tag(&self, id: &str, request: &UpdateTagRequest, tenant_id: &str) -> Result<Tag> {
        // Verify tag belongs to tenant before updating
        if let Some(tag) = self.db.find_tag_by_id(id).await? {
            if tag.tenant_id.as_ref() != Some(&tenant_id.to_string()) {
                return Err(Error::NotFound);
            }
        } else {
            return Err(Error::NotFound);
        }
        self.db.update_tag(id, request).await
    }

    pub async fn delete_tag(&self, id: &str, tenant_id: &str) -> Result<u64> {
        self.db.delete_tag(id, tenant_id).await
    }

    pub async fn find_all_tags(&self, params: &TagQuery) -> Result<Vec<Tag>> {
        self.db.find_tags(params).await
    }

    pub async fn count_tags(&self, params: &TagQuery) -> Result<i64> {
        self.db.count_tags(params).await
    }

    pub async fn find_tags_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<Vec<Tag>> {
        self.db.find_tags_by_target_id(target_id, tenant_id).await
    }

    pub async fn tag_exists_by_name_and_type(&self, name: &str, tag_type: &str, tenant_id: &str) -> Result<bool> {
        self.db.tag_exists_by_name_and_type(name, tag_type, tenant_id).await
    }

    pub async fn tag_exists_by_name_and_type_exclude_id(
        &self,
        name: &str,
        tag_type: &str,
        exclude_id: &str,
        tenant_id: &str,
    ) -> Result<bool> {
        self.db
            .tag_exists_by_name_and_type_exclude_id(name, tag_type, exclude_id, tenant_id)
            .await
    }

    pub async fn find_binding_by_id(&self, id: &str) -> Result<Option<TagBinding>> {
        self.db.find_binding_by_id(id).await
    }

    pub async fn create_binding(
        &self,
        request: &CreateTagBindingRequest,
        created_by: &str,
        tenant_id: &str,
    ) -> Result<TagBinding> {
        self.db.create_tag_binding(request, created_by, tenant_id).await
    }

    pub async fn delete_binding(&self, id: &str, tenant_id: &str) -> Result<u64> {
        self.db.delete_tag_binding(id, tenant_id).await
    }

    pub async fn delete_binding_by_tag_and_target(
        &self,
        tag_id: &str,
        target_id: &str,
        tenant_id: &str,
    ) -> Result<u64> {
        self.db
            .delete_tag_binding_by_tag_and_target(tag_id, target_id, tenant_id)
            .await
    }

    pub async fn find_bindings_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<Vec<TagBinding>> {
        self.db.find_bindings_by_tag_id(tag_id, tenant_id).await
    }

    pub async fn find_bindings_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<Vec<TagBinding>> {
        self.db.find_bindings_by_target_id(target_id, tenant_id).await
    }

    pub async fn count_bindings_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<i64> {
        self.db.count_bindings_by_tag_id(tag_id, tenant_id).await
    }

    pub async fn count_bindings_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<i64> {
        self.db.count_bindings_by_target_id(target_id, tenant_id).await
    }

    pub async fn binding_exists(&self, tag_id: &str, target_id: &str, tenant_id: &str) -> Result<bool> {
        self.db.tag_binding_exists(tag_id, target_id, tenant_id).await
    }

    pub async fn find_binding_by_tag_and_target(
        &self,
        tag_id: &str,
        target_id: &str,
        tenant_id: &str,
    ) -> Result<Option<TagBinding>> {
        self.db
            .find_binding_by_tag_and_target(tag_id, target_id, tenant_id)
            .await
    }

    pub async fn create_bindings_batch(
        &self,
        bindings: &[CreateTagBindingRequest],
        created_by: &str,
        tenant_id: &str,
    ) -> Result<Vec<TagBinding>> {
        self.db.create_tag_bindings_batch(bindings, created_by, tenant_id).await
    }

    pub async fn delete_all_bindings_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<u64> {
        self.db.delete_all_tag_bindings_by_target_id(target_id, tenant_id).await
    }

    pub async fn delete_all_bindings_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<u64> {
        self.db.delete_all_tag_bindings_by_tag_id(tag_id, tenant_id).await
    }
}
