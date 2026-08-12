use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListMemoriesQuery {
    pub agent_id: String,
    pub zone: Option<String>,
    pub source: Option<String>,
    pub limit: Option<u32>,
}
