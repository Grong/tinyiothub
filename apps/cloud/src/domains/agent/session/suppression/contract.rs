//! Typed contract for the generic suppression source state.
//!
//! A Source adapter reports one [`SourceSnapshot`] per capture. Snapshots are
//! opaque to the Harness apart from versions, refs and hashes. Business
//! semantics never enter these types.

// The typed error is the public prevention contract; boxing it would change
// every producer/consumer signature for a clippy size heuristic.
#![allow(clippy::result_large_err)]

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::canonical::{hash_payload, hash_state};
use super::error::SuppressionError;

/// Version of the generic Harness turn-state envelope.
pub const TURN_STATE_VERSION: u32 = 1;
/// Maximum length of an opaque revision in UTF-8 bytes.
pub const MAX_REVISION_BYTES: usize = 256;

const HASH_PREFIX: &str = "sha256:";

/// Adapter identity and schema version registered with the Harness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDescriptor {
    pub source_id: String,
    pub source_version: u32,
}

/// Immutable reference to a captured complete state projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateRef {
    pub id: String,
    pub revision: String,
    pub state_hash: String,
}

/// Immutable reference to a work item's payload projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadRef {
    pub id: String,
    pub revision: String,
    pub payload_hash: String,
}

/// Generic work item disposition. Business meaning stays in the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkItemDisposition {
    Active,
    Waiting,
    Closed,
}

/// One semantic work item in the current source snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub work_key: String,
    pub revision: String,
    pub disposition: WorkItemDisposition,
    pub payload_ref: PayloadRef,
    pub payload_hash: String,
}

/// Complete generic state projection for one registered source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub source_id: String,
    pub source_version: u32,
    pub revision: String,
    pub state_hash: String,
    pub available: bool,
    pub empty: bool,
    pub state_ref: Option<StateRef>,
    pub work_items: Vec<WorkItem>,
    pub error: Option<SuppressionError>,
}

/// Validated canonical bytes plus the revision/hash they belong to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalBytes {
    bytes: Vec<u8>,
    revision: String,
    hash: String,
}

impl CanonicalBytes {
    /// Wrap canonical source-state bytes.
    pub fn state(bytes: Vec<u8>, revision: impl Into<String>) -> Result<Self, SuppressionError> {
        Self::new(bytes, revision.into(), hash_state)
    }

    /// Wrap canonical work-payload bytes.
    pub fn payload(bytes: Vec<u8>, revision: impl Into<String>) -> Result<Self, SuppressionError> {
        Self::new(bytes, revision.into(), hash_payload)
    }

    fn new(bytes: Vec<u8>, revision: String, hash: fn(&[u8]) -> String) -> Result<Self, SuppressionError> {
        validate_revision(&revision)?;
        let hash = hash(&bytes);
        Ok(Self { bytes, revision, hash })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl WorkItem {
    /// Validate revisions, hashes and the embedded payload ref.
    pub fn validate(&self) -> Result<(), SuppressionError> {
        if self.work_key.is_empty() {
            return Err(contract_error(
                "source_contract_invalid",
                "workItem requires a non-empty workKey",
            ));
        }
        validate_revision(&self.revision)?;
        validate_hash(&self.payload_hash)?;
        if self.payload_ref.id.is_empty() {
            return Err(contract_error(
                "source_contract_invalid",
                "workItem payloadRef requires a non-empty id",
            ));
        }
        if self.payload_ref.revision != self.revision || self.payload_ref.payload_hash != self.payload_hash {
            return Err(contract_error(
                "source_ref_mismatch",
                "payloadRef does not match the workItem revision/payloadHash",
            ));
        }
        Ok(())
    }
}

impl SourceSnapshot {
    /// Validate this snapshot against its registered descriptor.
    pub fn validate(&self, descriptor: &SourceDescriptor) -> Result<(), SuppressionError> {
        if self.source_id != descriptor.source_id || self.source_version != descriptor.source_version {
            return Err(contract_error(
                "source_descriptor_mismatch",
                "snapshot sourceId/sourceVersion does not match the registered descriptor",
            ));
        }
        validate_revision(&self.revision)?;
        validate_hash(&self.state_hash)?;
        if self.available {
            self.validate_available()
        } else {
            self.validate_unavailable()
        }
    }

    fn validate_available(&self) -> Result<(), SuppressionError> {
        if self.error.is_some() {
            return Err(contract_error(
                "source_contract_invalid",
                "available source must not carry an error",
            ));
        }
        let state_ref = self
            .state_ref
            .as_ref()
            .ok_or_else(|| contract_error("source_contract_invalid", "available source requires a stateRef"))?;
        if state_ref.id.is_empty() {
            return Err(contract_error(
                "source_contract_invalid",
                "stateRef requires a non-empty id",
            ));
        }
        if state_ref.revision != self.revision || state_ref.state_hash != self.state_hash {
            return Err(contract_error(
                "source_ref_mismatch",
                "stateRef does not match the snapshot revision/stateHash",
            ));
        }
        if self.empty && !self.work_items.is_empty() {
            return Err(contract_error(
                "source_contract_invalid",
                "empty source must not carry workItems",
            ));
        }
        let mut seen = HashSet::new();
        for item in &self.work_items {
            if !seen.insert(item.work_key.as_str()) {
                return Err(contract_error(
                    "duplicate_work_key",
                    "snapshot contains duplicate workKey values",
                ));
            }
            item.validate()?;
        }
        Ok(())
    }

    fn validate_unavailable(&self) -> Result<(), SuppressionError> {
        if self.empty {
            return Err(contract_error(
                "source_contract_invalid",
                "unavailable source must report empty=false",
            ));
        }
        if self.state_ref.is_some() {
            return Err(contract_error(
                "source_contract_invalid",
                "unavailable source must not carry a stateRef",
            ));
        }
        if !self.work_items.is_empty() {
            return Err(contract_error(
                "source_contract_invalid",
                "unavailable source must not carry workItems",
            ));
        }
        let error = self
            .error
            .as_ref()
            .ok_or_else(|| contract_error("source_contract_invalid", "unavailable source requires an error"))?;
        if !is_stable_code(&error.code) {
            return Err(contract_error(
                "source_contract_invalid",
                "unavailable source requires a stable snake_case error code",
            ));
        }
        Ok(())
    }
}

/// Validate opaque revisions: 1..=256 UTF-8 bytes.
pub fn validate_revision(revision: &str) -> Result<(), SuppressionError> {
    let length = revision.len();
    if !(1..=MAX_REVISION_BYTES).contains(&length) {
        return Err(contract_error(
            "invalid_revision",
            format!("revision length {length} bytes is outside 1..={MAX_REVISION_BYTES}"),
        ));
    }
    Ok(())
}

/// Validate hashes: `sha256:` plus 64 lowercase hex characters.
pub fn validate_hash(hash: &str) -> Result<(), SuppressionError> {
    let valid = hash.strip_prefix(HASH_PREFIX).is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    });
    if !valid {
        return Err(contract_error(
            "invalid_hash",
            "hash must be sha256: followed by 64 lowercase hex characters",
        ));
    }
    Ok(())
}

/// Validate that no two descriptors share a sourceId.
pub fn validate_descriptors(descriptors: &[SourceDescriptor]) -> Result<(), SuppressionError> {
    let mut seen = HashSet::new();
    for descriptor in descriptors {
        if !seen.insert(descriptor.source_id.as_str()) {
            return Err(contract_error(
                "duplicate_source_id",
                "source registry contains duplicate sourceId values",
            ));
        }
    }
    Ok(())
}

fn is_stable_code(code: &str) -> bool {
    !code.is_empty()
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn contract_error(code: &str, what: impl Into<String>) -> SuppressionError {
    SuppressionError::new(code, what, "fix the source adapter contract and reconcile")
}

#[cfg(test)]
#[allow(clippy::module_inception)] // the test path must contain `suppression::contract`
mod contract {
    use serde_json::json;

    use crate::domains::agent::session::suppression::canonical::{
        canonical_bytes, canonicalize_json, hash_payload, hash_state, registry_version,
    };
    use crate::domains::agent::session::suppression::error::SuppressionError;

    use super::{
        CanonicalBytes, PayloadRef, SourceDescriptor, SourceSnapshot, StateRef, TURN_STATE_VERSION, WorkItem,
        WorkItemDisposition, validate_descriptors, validate_hash, validate_revision,
    };

    fn descriptor() -> SourceDescriptor {
        SourceDescriptor {
            source_id: "devices".into(),
            source_version: 1,
        }
    }

    fn work_item(work_key: &str, revision: &str) -> WorkItem {
        let payload_hash = hash_payload(b"payload");
        WorkItem {
            work_key: work_key.into(),
            revision: revision.into(),
            disposition: WorkItemDisposition::Active,
            payload_ref: PayloadRef {
                id: format!("payload-{work_key}"),
                revision: revision.into(),
                payload_hash: payload_hash.clone(),
            },
            payload_hash,
        }
    }

    fn available_snapshot(work_items: Vec<WorkItem>) -> SourceSnapshot {
        let revision = "rev-1".to_string();
        let state_hash = hash_state(b"state");
        SourceSnapshot {
            source_id: "devices".into(),
            source_version: 1,
            revision: revision.clone(),
            state_hash: state_hash.clone(),
            available: true,
            empty: work_items.is_empty(),
            state_ref: Some(StateRef {
                id: "state-1".into(),
                revision,
                state_hash,
            }),
            work_items,
            error: None,
        }
    }

    fn unavailable_fixture(code: &str) -> SourceSnapshot {
        SourceSnapshot {
            source_id: "devices".into(),
            source_version: 1,
            revision: "unavailable-1".into(),
            state_hash: hash_state(b"{}"),
            available: false,
            empty: false,
            state_ref: None,
            work_items: vec![],
            error: Some(SuppressionError::new(
                code,
                "capture devices source",
                "restore the source and reconcile",
            )),
        }
    }

    #[test]
    fn canonical_hash_is_stable_for_key_order_and_negative_zero() {
        let a = serde_json::json!({"b": -0.0, "a": "雪"});
        let b = serde_json::json!({"a": "雪", "b": 0});
        assert_eq!(canonical_bytes(&a).unwrap(), canonical_bytes(&b).unwrap());
        assert_eq!(hash_state(&canonical_bytes(&a).unwrap()).len(), 71);
    }

    #[test]
    fn unavailable_snapshot_has_safe_error_and_no_refs() {
        let snapshot = unavailable_fixture("source_unavailable");
        assert!(!snapshot.available);
        assert!(snapshot.state_ref.is_none());
        assert_eq!(snapshot.error.unwrap().code, "source_unavailable");
    }

    #[test]
    fn raw_json_rejects_duplicate_object_keys() {
        let error = canonicalize_json(br#"{"a":1,"a":2}"#).unwrap_err();
        assert_eq!(error.code, "canonical_duplicate_key");
    }

    #[test]
    fn raw_json_rejects_duplicate_object_keys_at_nested_depth() {
        let error = canonicalize_json(br#"{"outer":{"a":1,"a":2}}"#).unwrap_err();
        assert_eq!(error.code, "canonical_duplicate_key");
    }

    #[test]
    fn raw_json_rejects_non_finite_numbers() {
        for input in [b"NaN".as_slice(), b"Infinity", b"-Infinity", b"1e999"] {
            assert!(
                canonicalize_json(input).is_err(),
                "input {:?} must be rejected",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn raw_json_and_canonical_bytes_agree() {
        let value = json!({"b": -0.0, "a": "雪"});
        assert_eq!(
            canonicalize_json(r#"{"b":-0.0,"a":"雪"}"#.as_bytes()).unwrap(),
            canonical_bytes(&value).unwrap()
        );
    }

    #[test]
    fn jcs_number_golden_vectors() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (0.1, "0.1"),
            (-1.5, "-1.5"),
            (1234.5, "1234.5"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (-1e-7, "-1e-7"),
            (1e20, "100000000000000000000"),
            (1e21, "1e+21"),
            (-1e21, "-1e+21"),
            (1e-323, "1e-323"),
        ];
        for (value, expected) in cases {
            let value = serde_json::to_value(*value).unwrap();
            assert_eq!(
                canonical_bytes(&value).unwrap(),
                expected.as_bytes(),
                "unexpected JCS number for {value:?}"
            );
        }
    }

    #[test]
    fn jcs_string_escapes_and_unicode_are_deterministic() {
        let value = json!({"s": "雪😀\"\\\n\u{0001}", "c": "é"});
        let expected = r#"{"c":"é","s":"雪😀\"\\\n\u0001"}"#;
        assert_eq!(String::from_utf8(canonical_bytes(&value).unwrap()).unwrap(), expected);
    }

    #[test]
    fn canonically_equivalent_strings_share_nfc_bytes_and_hash() {
        let composed = json!({"s": "é"});
        let decomposed = json!({"s": "e\u{0301}"});
        let composed_bytes = canonical_bytes(&composed).unwrap();
        let decomposed_bytes = canonical_bytes(&decomposed).unwrap();
        assert_eq!(composed_bytes, decomposed_bytes);
        assert_eq!(hash_state(&composed_bytes), hash_state(&decomposed_bytes));
        assert_eq!(composed_bytes, r#"{"s":"é"}"#.as_bytes());
    }

    #[test]
    fn object_keys_sort_by_utf16_code_units() {
        let value = json!({"\u{E000}": 1, "\u{10000}": 2, "z": 3});
        let expected = format!("{{\"z\":3,\"{}\":2,\"{}\":1}}", '\u{10000}', '\u{E000}');
        assert_eq!(String::from_utf8(canonical_bytes(&value).unwrap()).unwrap(), expected);
    }

    #[test]
    fn arrays_preserve_order() {
        assert_eq!(
            canonical_bytes(&json!([3, 1, {"b": 1, "a": 2}])).unwrap(),
            br#"[3,1,{"a":2,"b":1}]"#.as_slice()
        );
    }

    #[test]
    fn hash_domains_are_separated() {
        assert!(hash_state(b"{}").starts_with("sha256:"));
        assert_eq!(hash_state(b"{}").len(), 71);
        assert_eq!(hash_payload(b"{}").len(), 71);
        assert_ne!(hash_state(b"{}"), hash_payload(b"{}"));
    }

    #[test]
    fn registry_version_is_order_independent_and_stable() {
        let left = SourceDescriptor {
            source_id: "alpha".into(),
            source_version: 1,
        };
        let right = SourceDescriptor {
            source_id: "beta".into(),
            source_version: 2,
        };
        assert_eq!(
            registry_version(&[left.clone(), right.clone()]),
            registry_version(&[right.clone(), left.clone()])
        );
        assert_eq!(registry_version(std::slice::from_ref(&left)).len(), 71);
        assert_ne!(
            registry_version(&[left]),
            registry_version(&[SourceDescriptor {
                source_id: "alpha".into(),
                source_version: 2,
            }])
        );
        assert_eq!(TURN_STATE_VERSION, 1);
    }

    #[test]
    fn registry_rejects_duplicate_source_ids() {
        assert!(validate_descriptors(&[descriptor()]).is_ok());
        let error = validate_descriptors(&[descriptor(), descriptor()]).unwrap_err();
        assert_eq!(error.code, "duplicate_source_id");
        let error = validate_descriptors(&[
            descriptor(),
            SourceDescriptor {
                source_id: "devices".into(),
                source_version: 2,
            },
        ])
        .unwrap_err();
        assert_eq!(error.code, "duplicate_source_id");
    }

    #[test]
    fn descriptor_and_snapshot_must_match() {
        let snapshot = available_snapshot(vec![]);
        assert!(snapshot.validate(&descriptor()).is_ok());
        let error = snapshot
            .validate(&SourceDescriptor {
                source_id: "other".into(),
                source_version: 1,
            })
            .unwrap_err();
        assert_eq!(error.code, "source_descriptor_mismatch");
        let error = snapshot
            .validate(&SourceDescriptor {
                source_id: "devices".into(),
                source_version: 2,
            })
            .unwrap_err();
        assert_eq!(error.code, "source_descriptor_mismatch");
    }

    #[test]
    fn unavailable_snapshot_shape_is_validated() {
        assert!(
            unavailable_fixture("source_unavailable")
                .validate(&descriptor())
                .is_ok()
        );

        let mut snapshot = unavailable_fixture("source_unavailable");
        snapshot.empty = true;
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );

        let mut snapshot = unavailable_fixture("source_unavailable");
        snapshot.state_ref = Some(StateRef {
            id: "state-1".into(),
            revision: snapshot.revision.clone(),
            state_hash: snapshot.state_hash.clone(),
        });
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );

        let mut snapshot = unavailable_fixture("source_unavailable");
        snapshot.work_items = vec![work_item("a", "r1")];
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );

        let mut snapshot = unavailable_fixture("source_unavailable");
        snapshot.error = None;
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );

        let mut snapshot = unavailable_fixture("not a stable code");
        snapshot.error = Some(SuppressionError::new(
            "not a stable code",
            "capture devices source",
            "restore the source",
        ));
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );
    }

    #[test]
    fn available_snapshot_requires_matching_refs() {
        let mut snapshot = available_snapshot(vec![]);
        snapshot.state_ref = None;
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );

        let mut snapshot = available_snapshot(vec![]);
        snapshot.state_ref.as_mut().unwrap().revision = "rev-2".into();
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_ref_mismatch"
        );

        let mut snapshot = available_snapshot(vec![]);
        snapshot.state_ref.as_mut().unwrap().state_hash = hash_state(b"other");
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_ref_mismatch"
        );

        let mut snapshot = available_snapshot(vec![]);
        snapshot.state_ref.as_mut().unwrap().id.clear();
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );
    }

    #[test]
    fn available_snapshot_rejects_error() {
        let mut snapshot = available_snapshot(vec![]);
        snapshot.error = Some(SuppressionError::new(
            "source_unavailable",
            "capture devices source",
            "restore the source",
        ));
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );
    }

    #[test]
    fn empty_available_snapshot_has_no_work_items() {
        let mut snapshot = available_snapshot(vec![]);
        snapshot.empty = true;
        assert!(snapshot.validate(&descriptor()).is_ok());
        snapshot.work_items = vec![work_item("a", "r1")];
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_contract_invalid"
        );
    }

    #[test]
    fn duplicate_work_keys_are_rejected() {
        let snapshot = available_snapshot(vec![work_item("a", "r1"), work_item("a", "r2")]);
        assert_eq!(snapshot.validate(&descriptor()).unwrap_err().code, "duplicate_work_key");
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(canonical_bytes(&value).unwrap_err().code, "duplicate_work_key");
    }

    #[test]
    fn revisions_are_one_to_256_utf8_bytes() {
        assert!(validate_revision("r").is_ok());
        assert!(validate_revision("").is_err());
        assert!(validate_revision(&"a".repeat(256)).is_ok());
        assert!(validate_revision(&"a".repeat(257)).is_err());
        assert!(validate_revision(&"雪".repeat(85)).is_ok());
        assert!(validate_revision(&"雪".repeat(86)).is_err());

        let mut snapshot = available_snapshot(vec![]);
        snapshot.revision.clear();
        assert_eq!(snapshot.validate(&descriptor()).unwrap_err().code, "invalid_revision");
    }

    #[test]
    fn hashes_require_sha256_prefix_and_lowercase_hex() {
        let good = hash_state(b"x");
        assert!(validate_hash(&good).is_ok());
        assert!(validate_hash("sha256:").is_err());
        assert!(validate_hash(&good.to_uppercase()).is_err());
        assert!(validate_hash(&format!("sha256:{}", "a".repeat(63))).is_err());
        assert!(validate_hash(&format!("sha256:{}", "a".repeat(65))).is_err());
        assert!(validate_hash(&format!("sha256:{}", "A".repeat(64))).is_err());
        assert!(validate_hash(&format!("sha256:{}g", "a".repeat(63))).is_err());
        assert!(validate_hash(good.strip_prefix("sha256:").unwrap()).is_err());
    }

    #[test]
    fn payload_refs_must_match_work_item() {
        let mut snapshot = available_snapshot(vec![work_item("a", "r1")]);
        snapshot.work_items[0].payload_ref.revision = "r2".into();
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_ref_mismatch"
        );

        let mut snapshot = available_snapshot(vec![work_item("a", "r1")]);
        snapshot.work_items[0].payload_ref.payload_hash = hash_payload(b"other");
        assert_eq!(
            snapshot.validate(&descriptor()).unwrap_err().code,
            "source_ref_mismatch"
        );

        let mut snapshot = available_snapshot(vec![work_item("a", "r1")]);
        snapshot.work_items[0].payload_hash = "not-a-hash".into();
        assert_eq!(snapshot.validate(&descriptor()).unwrap_err().code, "invalid_hash");
    }

    #[test]
    fn work_item_arrays_sort_by_work_key() {
        let left = available_snapshot(vec![work_item("b", "r1"), work_item("a", "r1")]);
        let right = available_snapshot(vec![work_item("a", "r1"), work_item("b", "r1")]);
        assert_eq!(
            canonical_bytes(&serde_json::to_value(&left).unwrap()).unwrap(),
            canonical_bytes(&serde_json::to_value(&right).unwrap()).unwrap()
        );
    }

    #[test]
    fn canonical_bytes_newtype_carries_revision_and_hash() {
        let state = CanonicalBytes::state(b"{}".to_vec(), "rev-1").unwrap();
        assert_eq!(state.revision(), "rev-1");
        assert_eq!(state.hash(), hash_state(b"{}").as_str());
        assert_eq!(state.as_bytes(), b"{}".as_slice());

        let payload = CanonicalBytes::payload(b"data".to_vec(), "rev-2").unwrap();
        assert_eq!(payload.hash(), hash_payload(b"data").as_str());
        assert_eq!(payload.into_bytes(), b"data".to_vec());

        assert_eq!(
            CanonicalBytes::state(b"{}".to_vec(), "").unwrap_err().code,
            "invalid_revision"
        );
    }
}
