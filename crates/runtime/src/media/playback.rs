use serde::{Deserialize, Serialize};
use tinyiothub_core::media::EvidenceRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackDescriptor {
    pub evidence_id: String,
    pub mime: String,
    pub source_seq: u64,
    pub media_time: chrono::DateTime<chrono::Utc>,
}

impl From<EvidenceRef> for PlaybackDescriptor {
    fn from(reference: EvidenceRef) -> Self {
        Self {
            evidence_id: reference.evidence_id,
            mime: reference.mime,
            source_seq: reference.source_seq,
            media_time: reference.media_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use tinyiothub_core::media::EvidenceRef;

    #[test]
    fn playback_descriptor_contains_reference_not_bytes() {
        let reference = EvidenceRef {
            evidence_id: "e1".into(),
            mime: "image/png".into(),
            source_seq: 2,
            media_time: chrono::Utc::now(),
        };
        let descriptor = super::PlaybackDescriptor::from(reference);
        let json = serde_json::to_string(&descriptor).expect("descriptor");
        assert!(json.contains("e1"));
        assert!(!json.contains("bytes"));
    }
}
