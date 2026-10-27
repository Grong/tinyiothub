use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(with = "String")]
pub enum DataKind {
    #[default]
    Values,
    Detections,
    Image,
    Video,
    Custom(String),
}

impl Serialize for DataKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Values => "values",
            Self::Detections => "detections",
            Self::Image => "image",
            Self::Video => "video",
            Self::Custom(value) => value,
        })
    }
}

impl<'de> Deserialize<'de> for DataKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "values" => Self::Values,
            "detections" => Self::Detections,
            "image" => Self::Image,
            "video" => Self::Video,
            _ => Self::Custom(value),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPayload {
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub v: u8,
    pub source: String,
    #[serde(default)]
    pub data_kind: DataKind,
    pub seq: u64,
    pub time: DateTime<Utc>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub media: Option<MediaPayload>,
    #[serde(default)]
    pub error: Option<SourceError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAttachment {
    pub evidence_id: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub source_seq: u64,
    pub media_time: DateTime<Utc>,
}

impl From<&MediaAttachment> for EvidenceRef {
    fn from(attachment: &MediaAttachment) -> Self {
        Self {
            evidence_id: attachment.evidence_id.clone(),
            mime: attachment.mime.clone(),
            source_seq: attachment.source_seq,
            media_time: attachment.media_time,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageInput {
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub job_id: Option<String>,
    pub device_id: String,
    pub source_id: String,
    pub source_seq: u64,
    pub media_time: DateTime<Utc>,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub max_age: std::time::Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRef {
    pub evidence_id: String,
    pub mime: String,
    pub source_seq: u64,
    pub media_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaEvidenceRecord {
    pub id: String,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub job_id: Option<String>,
    pub device_id: String,
    pub source_id: String,
    pub source_seq: u64,
    pub media_time: DateTime<Utc>,
    pub mime: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub storage_path: String,
    pub dedup_key: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    #[error("unsupported media MIME: {0}")]
    UnsupportedMime(String),
    #[error("image exceeds 2 MiB")]
    ImageTooLarge,
    #[error("image dimensions exceed 16 MP")]
    ImageDimensions,
    #[error("invalid image or MIME/magic mismatch")]
    InvalidImage,
    #[error("media is stale or from the future")]
    StaleMedia,
    #[error("one tool call may carry at most one image")]
    TooManyImages,
    #[error("run image budget exceeds 8 MiB")]
    RunImageBudget,
    #[error("video_is_for_human_not_model")]
    VideoIsForHumanNotModel,
    #[error("media validation failed: {0}")]
    Validation(String),
    #[error("media storage failed: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_kind_schema_matches_its_string_wire_format() {
        let schema = serde_json::to_value(schemars::schema_for!(super::DataKind)).expect("schema");
        assert_eq!(
            schema,
            serde_json::json!({"$schema":"https://json-schema.org/draft/2020-12/schema","title":"string","type":"string"})
        );
    }
}
