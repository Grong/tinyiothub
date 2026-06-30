//! Knowledge Graph — entities and relationships available to Agent heartbeat context.
//!
//! AI crate defines types and the KnowledgeGraph trait. Cloud implements with
//! SQLite (workspace device registry, entity index) and injects query results
//! into the heartbeat prompt when relevant.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A node in the knowledge graph (device, workspace, user, concept).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntity {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub properties: Option<serde_json::Value>,
    pub workspace_id: Option<String>,
}

/// A directed edge between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelation {
    pub id: String,
    pub from_entity_id: String,
    pub to_entity_id: String,
    pub relation_type: String,
    pub properties: Option<serde_json::Value>,
}

/// Query interface for the knowledge graph.
/// Cloud implements with SQLite (device registry, entity index).
#[async_trait]
pub trait KnowledgeGraph: Send + Sync {
    /// Search entities by name or type, scoped to a workspace.
    async fn search_entities(
        &self,
        workspace_id: &str,
        query: &str,
        entity_type: Option<&str>,
        limit: u32,
    ) -> Vec<KnowledgeEntity>;

    /// Get all relations for an entity (both directions).
    async fn get_relations(&self, entity_id: &str) -> Vec<KnowledgeRelation>;

    /// Get entities directly related to the given entity.
    async fn get_related_entities(&self, entity_id: &str, relation_type: Option<&str>) -> Vec<KnowledgeEntity>;

    /// Look up a single entity by id.
    async fn get_entity(&self, entity_id: &str) -> Option<KnowledgeEntity>;
}

/// No-op implementation for testing / when knowledge graph isn't configured.
pub struct NoopKnowledgeGraph;

#[async_trait]
impl KnowledgeGraph for NoopKnowledgeGraph {
    async fn search_entities(
        &self,
        _workspace_id: &str,
        _query: &str,
        _entity_type: Option<&str>,
        _limit: u32,
    ) -> Vec<KnowledgeEntity> {
        vec![]
    }

    async fn get_relations(&self, _entity_id: &str) -> Vec<KnowledgeRelation> {
        vec![]
    }

    async fn get_related_entities(&self, _entity_id: &str, _relation_type: Option<&str>) -> Vec<KnowledgeEntity> {
        vec![]
    }

    async fn get_entity(&self, _entity_id: &str) -> Option<KnowledgeEntity> {
        None
    }
}
