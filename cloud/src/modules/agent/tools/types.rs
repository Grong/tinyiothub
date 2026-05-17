// ToolDef, ToolGroup, ToolCatalog types
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    pub label: String,
    pub description: String,
    pub danger: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolGroup {
    pub id: String,
    pub label: String,
    pub source: String,
    pub tools: Vec<ToolDef>,
}
