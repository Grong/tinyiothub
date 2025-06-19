use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use super::tag::TagResponse;

#[derive(Debug, Deserialize, Serialize)]
pub struct AppResponse {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub tags: Vec<TagResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppDetailWithSiteResponse {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub mode: String,
    pub enable_site: bool,
    pub enable_api: bool,
    pub api_rpm: i32,
    pub api_rph: i32,
    pub tracing: String,
    pub api_base_url: String,
    pub site: SiteResponse,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub created_by: i32,
    pub updated_by: i32,
    pub access_mode: String,
    pub icon_type: String,
    pub icon: String,
    pub icon_background: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SiteResponse {
    pub access_token: String,
    pub title: String,
    pub description: String,
    pub icon_type: String,
    pub icon: String,
    pub icon_background: String,
    pub icon_url: String,
    pub app_base_url: String,
}