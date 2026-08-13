//! tj_* API Key 令牌机制（HMAC-SHA256 签名）。

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn sign_payload(payload: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, result.into_bytes())
}

pub fn verify_signature(payload: &str, signature: &str, secret: &str) -> bool {
    use subtle::ConstantTimeEq;
    let expected = sign_payload(payload, secret);
    expected.as_bytes().ct_eq(signature.as_bytes()).into()
}

pub fn generate_token(secret: &str, tenant_id: &str, user_id: &str) -> String {
    let exp = chrono::Utc::now().timestamp() + 86400 * 7;
    let payload = serde_json::json!({
        "tenant_id": tenant_id,
        "user_id": user_id,
        "exp": exp,
    });

    let payload_str = payload.to_string();
    let signature = sign_payload(&payload_str, secret);

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, payload_str);

    format!("tj_{}:{}", encoded, signature)
}
