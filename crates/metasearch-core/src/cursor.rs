use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorPayload {
    pub query_hash: String,
    pub page: u32,
    #[serde(default)]
    pub engine_cursors: BTreeMap<String, String>,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    InvalidEncoding,
    InvalidSignature,
    InvalidPayload,
    Expired,
}

impl Display for CursorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEncoding => formatter.write_str("cursor encoding is invalid"),
            Self::InvalidSignature => formatter.write_str("cursor signature is invalid"),
            Self::InvalidPayload => formatter.write_str("cursor payload is invalid"),
            Self::Expired => formatter.write_str("cursor has expired"),
        }
    }
}

impl std::error::Error for CursorError {}

pub struct CursorSigner<'a> {
    key: &'a [u8],
}

impl<'a> CursorSigner<'a> {
    pub const fn new(key: &'a [u8]) -> Self {
        Self { key }
    }

    pub fn encode(&self, payload: &CursorPayload) -> Result<String, CursorError> {
        let bytes = serde_json::to_vec(payload).map_err(|_| CursorError::InvalidPayload)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.key).map_err(|_| CursorError::InvalidPayload)?;
        mac.update(&bytes);
        let signature = mac.finalize().into_bytes();
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(bytes),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    pub fn decode(&self, cursor: &str, now_ms: u64) -> Result<CursorPayload, CursorError> {
        let (payload, signature) = cursor.split_once('.').ok_or(CursorError::InvalidEncoding)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.key).map_err(|_| CursorError::InvalidPayload)?;
        mac.update(&bytes);
        mac.verify_slice(&signature)
            .map_err(|_| CursorError::InvalidSignature)?;
        let payload: CursorPayload =
            serde_json::from_slice(&bytes).map_err(|_| CursorError::InvalidPayload)?;
        if payload.expires_at_ms <= now_ms {
            return Err(CursorError::Expired);
        }
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_validates_cursor() {
        let signer = CursorSigner::new(b"test signing key");
        let payload = CursorPayload {
            query_hash: "query-hash".into(),
            page: 2,
            engine_cursors: BTreeMap::new(),
            expires_at_ms: 2_000,
        };
        let encoded = signer.encode(&payload).unwrap();
        assert_eq!(signer.decode(&encoded, 1_000).unwrap(), payload);
    }

    #[test]
    fn rejects_tampering() {
        let signer = CursorSigner::new(b"test signing key");
        let payload = CursorPayload {
            query_hash: "query-hash".into(),
            page: 2,
            engine_cursors: BTreeMap::new(),
            expires_at_ms: 2_000,
        };
        let mut encoded = signer.encode(&payload).unwrap();
        encoded.push('x');
        assert!(matches!(
            signer.decode(&encoded, 1_000),
            Err(CursorError::InvalidSignature | CursorError::InvalidEncoding)
        ));
    }
}
