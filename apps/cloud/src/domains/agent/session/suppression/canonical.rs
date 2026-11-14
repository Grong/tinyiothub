//! Canonical JSON bytes and domain-separated hashes (JCS/RFC 8785).
//!
//! Object keys sort by UTF-16 code units, numbers use the shortest
//! ECMAScript round-trip form, `-0` becomes `0`, only finite numbers are
//! accepted, and strings use the JCS escape set with raw UTF-8 for everything
//! else. Arrays keep their order except `workItems`, which sort by
//! `(workKey, revision)`.
//!
//! Object keys and string values are normalized to Unicode NFC at the input
//! boundary, so ordering, duplicate detection and emission all use the same
//! form and canonically equivalent inputs produce identical bytes and hashes.

// The typed error is the public prevention contract; boxing it would change
// every producer/consumer signature for a clippy size heuristic.
#![allow(clippy::result_large_err)]

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Write as _;

use serde::Deserialize;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use super::contract::{SourceDescriptor, TURN_STATE_VERSION};
use super::error::SuppressionError;

/// Domain prefix for canonical source-state bytes.
pub const STATE_DOMAIN: &[u8] = b"tinyiothub:source-state:v1\0";
/// Domain prefix for canonical work-payload bytes.
pub const PAYLOAD_DOMAIN: &[u8] = b"tinyiothub:work-payload:v1\0";
/// Domain prefix for the canonical source-registry list.
pub const REGISTRY_DOMAIN: &[u8] = b"tinyiothub:source-registry:v1\0";

const DUPLICATE_KEY_MARKER: &str = "__tinyiothub_duplicate_key__";
const NON_FINITE_MARKER: &str = "__tinyiothub_non_finite__";

/// Canonicalize an already validated JSON value into JCS bytes.
///
/// Strings and object keys are normalized to NFC first so that ordering,
/// duplicate detection and emission all operate on the same form.
pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>, SuppressionError> {
    let value = normalize_value(value, None)?;
    let mut out = String::new();
    write_value(&mut out, &value, None)?;
    Ok(out.into_bytes())
}

fn normalize_value(value: &Value, key: Option<&str>) -> Result<Value, SuppressionError> {
    Ok(match value {
        Value::String(string) => Value::String(string.nfc().collect()),
        Value::Array(items) => {
            let items: Vec<Value> = items
                .iter()
                .map(|item| normalize_value(item, None))
                .collect::<Result<_, _>>()?;
            if key == Some("workItems") {
                reject_duplicate_work_keys(&items)?;
            }
            Value::Array(items)
        }
        Value::Object(object) => {
            let mut normalized = Map::with_capacity(object.len());
            for (key, value) in object {
                let key: String = key.nfc().collect();
                let value = normalize_value(value, Some(&key))?;
                if normalized.insert(key, value).is_some() {
                    return Err(canonical_error(
                        "canonical_duplicate_key",
                        "canonicalize object keys that collide after NFC normalization",
                    ));
                }
            }
            Value::Object(normalized)
        }
        other => other.clone(),
    })
}

fn reject_duplicate_work_keys(items: &[Value]) -> Result<(), SuppressionError> {
    let mut seen = HashSet::new();
    for item in items {
        if let Some(work_key) = work_item_field(item, "workKey")
            && !seen.insert(work_key)
        {
            return Err(canonical_error(
                "duplicate_work_key",
                "canonicalize workItems with canonically equivalent workKey values",
            ));
        }
    }
    Ok(())
}

/// Parse raw JSON with duplicate-key detection, then canonicalize it.
pub fn canonicalize_json(input: &[u8]) -> Result<Vec<u8>, SuppressionError> {
    let value: CheckedValue = serde_json::from_slice(input).map_err(map_parse_error)?;
    canonical_bytes(&value.0)
}

/// SHA-256 over `tinyiothub:source-state:v1\0` plus canonical bytes.
pub fn hash_state(bytes: &[u8]) -> String {
    hash_with_domain(STATE_DOMAIN, bytes)
}

/// SHA-256 over `tinyiothub:work-payload:v1\0` plus canonical bytes.
pub fn hash_payload(bytes: &[u8]) -> String {
    hash_with_domain(PAYLOAD_DOMAIN, bytes)
}

/// SHA-256 over the sorted descriptor list and the turn-state envelope version.
pub fn registry_version(descriptors: &[SourceDescriptor]) -> String {
    let mut sorted: Vec<&SourceDescriptor> = descriptors.iter().collect();
    sorted.sort_by(|left, right| {
        cmp_utf16(&left.source_id, &right.source_id).then(left.source_version.cmp(&right.source_version))
    });
    let value = Value::Array(
        sorted
            .into_iter()
            .map(|descriptor| {
                let mut object = Map::new();
                object.insert("sourceId".to_string(), Value::String(descriptor.source_id.clone()));
                object.insert("sourceVersion".to_string(), Value::from(descriptor.source_version));
                object.insert("stateVersion".to_string(), Value::from(TURN_STATE_VERSION));
                Value::Object(object)
            })
            .collect(),
    );
    let bytes = canonical_bytes(&value).expect("registry descriptors are always canonicalizable");
    hash_with_domain(REGISTRY_DOMAIN, &bytes)
}

fn hash_with_domain(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn write_value(out: &mut String, value: &Value, key: Option<&str>) -> Result<(), SuppressionError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(boolean) => out.push_str(if *boolean { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&number_to_jcs(number)?),
        Value::String(string) => write_string(out, string),
        Value::Array(items) if key == Some("workItems") => write_work_items(out, items)?,
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(out, item, None)?;
            }
            out.push(']');
        }
        Value::Object(object) => {
            out.push('{');
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort_by(|left, right| cmp_utf16(left, right));
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, key.as_str());
                out.push(':');
                let value = object.get(*key).expect("object key came from the object");
                write_value(out, value, Some(key.as_str()))?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_work_items(out: &mut String, items: &[Value]) -> Result<(), SuppressionError> {
    let mut sorted: Vec<&Value> = items.iter().collect();
    sorted.sort_by(|left, right| compare_work_items(left, right));
    for pair in sorted.windows(2) {
        let left = work_item_field(pair[0], "workKey");
        let right = work_item_field(pair[1], "workKey");
        if left.is_some() && left == right {
            return Err(canonical_error(
                "duplicate_work_key",
                "canonicalize workItems with duplicate workKey values",
            ));
        }
    }
    out.push('[');
    for (index, item) in sorted.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_value(out, item, None)?;
    }
    out.push(']');
    Ok(())
}

fn compare_work_items(left: &Value, right: &Value) -> Ordering {
    let left_key = work_item_field(left, "workKey").unwrap_or("");
    let right_key = work_item_field(right, "workKey").unwrap_or("");
    cmp_utf16(left_key, right_key).then_with(|| {
        let left_revision = work_item_field(left, "revision").unwrap_or("");
        let right_revision = work_item_field(right, "revision").unwrap_or("");
        cmp_utf16(left_revision, right_revision)
    })
}

fn work_item_field<'a>(item: &'a Value, field: &str) -> Option<&'a str> {
    item.as_object()?.get(field)?.as_str()
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.nfc() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{000C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

fn number_to_jcs(number: &Number) -> Result<String, SuppressionError> {
    if let Some(integer) = number.as_i64() {
        return Ok(integer.to_string());
    }
    if let Some(integer) = number.as_u64() {
        return Ok(integer.to_string());
    }
    let value = number
        .as_f64()
        .ok_or_else(|| canonical_error("canonical_non_finite", "canonicalize a non-finite number"))?;
    if !value.is_finite() {
        return Err(canonical_error(
            "canonical_non_finite",
            "canonicalize a non-finite number",
        ));
    }
    Ok(format_f64_jcs(value))
}

fn format_f64_jcs(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let negative = value.is_sign_negative();
    let magnitude = value.abs();
    let rendered = format!("{magnitude:e}");
    let (mantissa, exponent) = rendered
        .split_once('e')
        .expect("LowerExp formatting always contains an exponent");
    let exponent: i32 = exponent.parse().expect("LowerExp exponent is an integer");
    let digits: String = mantissa.chars().filter(|character| *character != '.').collect();
    let digit_count = digits.len() as i32;
    let point = exponent + 1;

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if digit_count <= point && point <= 21 {
        out.push_str(&digits);
        for _ in 0..(point - digit_count) {
            out.push('0');
        }
    } else if 0 < point && point <= 21 {
        let split = point as usize;
        out.push_str(&digits[..split]);
        out.push('.');
        out.push_str(&digits[split..]);
    } else if -6 < point && point <= 0 {
        out.push_str("0.");
        for _ in 0..(-point) {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if digit_count > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        let written_exponent = point - 1;
        if written_exponent >= 0 {
            out.push('+');
        }
        out.push_str(&written_exponent.to_string());
    }
    out
}

fn cmp_utf16(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn canonical_error(code: &str, what: &str) -> SuppressionError {
    SuppressionError::new(
        code,
        what,
        "supply finite, duplicate-free JSON values and stable work keys",
    )
}

fn map_parse_error(error: serde_json::Error) -> SuppressionError {
    let message = error.to_string();
    if message.contains(DUPLICATE_KEY_MARKER) {
        return canonical_error(
            "canonical_duplicate_key",
            "parse raw JSON containing duplicate object keys",
        );
    }
    if message.contains(NON_FINITE_MARKER) || message.contains("number out of range") {
        return canonical_error("canonical_non_finite", "parse raw JSON containing a non-finite number");
    }
    canonical_error("canonical_invalid_json", "parse raw JSON")
}

struct CheckedValue(Value);

impl<'de> Deserialize<'de> for CheckedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(CheckedVisitor)
    }
}

struct CheckedVisitor;

impl<'de> Visitor<'de> for CheckedVisitor {
    type Value = CheckedValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Number(Number::from(value))))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Number(Number::from(value))))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(|number| CheckedValue(Value::Number(number)))
            .ok_or_else(|| E::custom(NON_FINITE_MARKER))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::String(value.to_string())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::String(value)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Null))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Null))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        CheckedValue::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element::<CheckedValue>()? {
            items.push(item.0);
        }
        Ok(CheckedValue(Value::Array(items)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        while let Some((key, value)) = map.next_entry::<String, CheckedValue>()? {
            if object.contains_key(&key) {
                return Err(de::Error::custom(DUPLICATE_KEY_MARKER));
            }
            object.insert(key, value.0);
        }
        Ok(CheckedValue(Value::Object(object)))
    }
}
