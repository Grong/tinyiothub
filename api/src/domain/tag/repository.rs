use async_trait::async_trait;

use crate::dto::entity::tag::{
    CreateTagBindingRequest, CreateTagRequest, Tag, TagBinding,
    TagQuery, UpdateTagRequest,
};
use crate::shared::error::Result;

#[async_trait]
pub trait TagRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Tag>>;
    async fn find_by_name_and_type(&self, name: &str, tag_type: &str) -> Result<Option<Tag>>;
    async fn create(&self, request: &CreateTagRequest, created_by: &str, tenant_id: &str) -> Result<Tag>;
    async fn update(&self, id: &str, request: &UpdateTagRequest) -> Result<Tag>;
    async fn delete(&self, id: &str, tenant_id: &str) -> Result<u64>;
    async fn find_all(&self, params: &TagQuery) -> Result<Vec<Tag>>;
    async fn count(&self, params: &TagQuery) -> Result<i64>;
    async fn find_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<Vec<Tag>>;
    async fn exists_by_name_and_type(&self, name: &str, tag_type: &str, tenant_id: &str) -> Result<bool>;
    async fn exists_by_name_and_type_exclude_id(
        &self,
        name: &str,
        tag_type: &str,
        exclude_id: &str,
        tenant_id: &str,
    ) -> Result<bool>;
}

#[async_trait]
pub trait TagBindingRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<TagBinding>>;
    async fn create(&self, request: &CreateTagBindingRequest, created_by: &str, tenant_id: &str) -> Result<TagBinding>;
    async fn delete(&self, id: &str, tenant_id: &str) -> Result<u64>;
    async fn delete_by_tag_and_target(&self, tag_id: &str, target_id: &str, tenant_id: &str) -> Result<u64>;
    async fn find_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<Vec<TagBinding>>;
    async fn find_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<Vec<TagBinding>>;
    async fn count_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<i64>;
    async fn count_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<i64>;
    async fn exists(&self, tag_id: &str, target_id: &str, tenant_id: &str) -> Result<bool>;
    async fn find_by_tag_and_target(&self, tag_id: &str, target_id: &str, tenant_id: &str) -> Result<Option<TagBinding>>;
    async fn create_batch(
        &self,
        bindings: &[CreateTagBindingRequest],
        created_by: &str,
        tenant_id: &str,
    ) -> Result<Vec<TagBinding>>;
    async fn delete_all_by_target_id(&self, target_id: &str, tenant_id: &str) -> Result<u64>;
    async fn delete_all_by_tag_id(&self, tag_id: &str, tenant_id: &str) -> Result<u64>;
}
