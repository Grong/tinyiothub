use std::sync::Arc;

use crate::dto::entity::tag::{
    BatchTagBindingRequest, CreateTagBindingRequest, CreateTagRequest, Tag, TagBinding,
    TagQuery, UpdateTagRequest,
};
use crate::shared::error::Result;

use super::repository::{TagBindingRepository, TagRepository};

pub struct TagService {
    tag_repository: Arc<dyn TagRepository>,
    tag_binding_repository: Arc<dyn TagBindingRepository>,
}

impl TagService {
    pub fn new(
        tag_repository: Arc<dyn TagRepository>,
        tag_binding_repository: Arc<dyn TagBindingRepository>,
    ) -> Self {
        Self {
            tag_repository,
            tag_binding_repository,
        }
    }

    pub async fn find_tag_by_id(&self, id: &str) -> Result<Option<Tag>> {
        self.tag_repository.find_by_id(id).await
    }

    pub async fn find_tag_by_name_and_type(
        &self,
        name: &str,
        tag_type: &str,
    ) -> Result<Option<Tag>> {
        self.tag_repository.find_by_name_and_type(name, tag_type).await
    }

    pub async fn create_tag(&self, request: &CreateTagRequest, created_by: &str) -> Result<Tag> {
        self.tag_repository.create(request, created_by).await
    }

    pub async fn update_tag(&self, id: &str, request: &UpdateTagRequest) -> Result<Tag> {
        self.tag_repository.update(id, request).await
    }

    pub async fn delete_tag(&self, id: &str) -> Result<u64> {
        self.tag_repository.delete(id).await
    }

    pub async fn find_all_tags(&self, params: &TagQuery) -> Result<Vec<Tag>> {
        self.tag_repository.find_all(params).await
    }

    pub async fn count_tags(&self, params: &TagQuery) -> Result<i64> {
        self.tag_repository.count(params).await
    }

    pub async fn find_tags_by_target_id(&self, target_id: &str) -> Result<Vec<Tag>> {
        self.tag_repository.find_by_target_id(target_id).await
    }

    pub async fn tag_exists_by_name_and_type(
        &self,
        name: &str,
        tag_type: &str,
    ) -> Result<bool> {
        self.tag_repository.exists_by_name_and_type(name, tag_type).await
    }

    pub async fn tag_exists_by_name_and_type_exclude_id(
        &self,
        name: &str,
        tag_type: &str,
        exclude_id: &str,
    ) -> Result<bool> {
        self.tag_repository
            .exists_by_name_and_type_exclude_id(name, tag_type, exclude_id)
            .await
    }

    pub async fn find_binding_by_id(&self, id: &str) -> Result<Option<TagBinding>> {
        self.tag_binding_repository.find_by_id(id).await
    }

    pub async fn create_binding(
        &self,
        request: &CreateTagBindingRequest,
        created_by: &str,
    ) -> Result<TagBinding> {
        self.tag_binding_repository.create(request, created_by).await
    }

    pub async fn delete_binding(&self, id: &str) -> Result<u64> {
        self.tag_binding_repository.delete(id).await
    }

    pub async fn delete_binding_by_tag_and_target(
        &self,
        tag_id: &str,
        target_id: &str,
    ) -> Result<u64> {
        self.tag_binding_repository
            .delete_by_tag_and_target(tag_id, target_id)
            .await
    }

    pub async fn find_bindings_by_tag_id(&self, tag_id: &str) -> Result<Vec<TagBinding>> {
        self.tag_binding_repository.find_by_tag_id(tag_id).await
    }

    pub async fn find_bindings_by_target_id(&self, target_id: &str) -> Result<Vec<TagBinding>> {
        self.tag_binding_repository.find_by_target_id(target_id).await
    }

    pub async fn count_bindings_by_tag_id(&self, tag_id: &str) -> Result<i64> {
        self.tag_binding_repository.count_by_tag_id(tag_id).await
    }

    pub async fn count_bindings_by_target_id(&self, target_id: &str) -> Result<i64> {
        self.tag_binding_repository.count_by_target_id(target_id).await
    }

    pub async fn binding_exists(&self, tag_id: &str, target_id: &str) -> Result<bool> {
        self.tag_binding_repository.exists(tag_id, target_id).await
    }

    pub async fn create_bindings_batch(
        &self,
        bindings: &[CreateTagBindingRequest],
        created_by: &str,
    ) -> Result<Vec<TagBinding>> {
        self.tag_binding_repository.create_batch(bindings, created_by).await
    }

    pub async fn delete_all_bindings_by_target_id(&self, target_id: &str) -> Result<u64> {
        self.tag_binding_repository.delete_all_by_target_id(target_id).await
    }

    pub async fn delete_all_bindings_by_tag_id(&self, tag_id: &str) -> Result<u64> {
        self.tag_binding_repository.delete_all_by_tag_id(tag_id).await
    }
}
