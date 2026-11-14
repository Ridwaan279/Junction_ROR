//! Signature verification and durable-inbox boundary; A mounts the HTTP route and owns storage.
use chrono::{DateTime, Utc};
use emberwatch_domain::SourceError;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;
use std::{future::Future, pin::Pin};

pub struct WebhookRequest<'a> {
    pub body: &'a [u8],
    pub signature: &'a str,
    pub event_id: &'a str,
    pub event_type: &'a str,
    pub now: DateTime<Utc>,
}
/// This binding is trusted server configuration, never selected using an unverified request.
pub struct WebhookBinding {
    pub source_id: String,
    pub expected_upstream_id: String,
    pub upstream_id_pointer: String,
    pub allowed_events: Vec<String>,
}
pub struct WebhookReceiver {
    secret: String,
    binding: WebhookBinding,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedWebhook {
    pub id: String,
    pub event: String,
    pub created_at: DateTime<Utc>,
    pub source_id: String,
    pub data: Value,
    pub body_hash: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptDisposition {
    Inserted,
    Duplicate,
}
pub type InboxFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ReceiptDisposition, SourceError>> + Send + 'a>>;
pub trait WebhookInbox: Send + Sync {
    /// Atomically persist before returning success; unique key is (source_id, id).
    /// Duplicate IDs with a different body_hash must fail rather than overwrite the original.
    fn persist_once<'a>(&'a self, event: &'a VerifiedWebhook) -> InboxFuture<'a>;
}
impl WebhookReceiver {
    pub fn new(secret: String, binding: WebhookBinding) -> Result<Self, SourceError> {
        if secret.is_empty()
            || binding.source_id.is_empty()
            || binding.expected_upstream_id.is_empty()
            || !binding.upstream_id_pointer.starts_with('/')
            || binding.allowed_events.is_empty()
        {
            return Err(crate::error(
                "WEBHOOK_CONFIG",
                "A secret and independently configured upstream binding are required.",
            ));
        }
        Ok(Self { secret, binding })
    }
    pub fn verify(&self, request: WebhookRequest<'_>) -> Result<VerifiedWebhook, SourceError> {
        if request.body.len() > 1024 * 1024 {
            return Err(crate::error(
                "WEBHOOK_TOO_LARGE",
                "Webhook body exceeds 1 MiB.",
            ));
        }
        let mut parts = HashMap::new();
        for pair in request.signature.split(',') {
            let (key, value) = pair
                .trim()
                .split_once('=')
                .ok_or_else(|| crate::error("WEBHOOK_SIGNATURE", "Malformed signature header."))?;
            if value.contains('=') || parts.insert(key, value).is_some() {
                return Err(crate::error(
                    "WEBHOOK_SIGNATURE",
                    "Duplicate or malformed signature fields.",
                ));
            }
        }
        let timestamp = parts
            .get("t")
            .filter(|s| !s.is_empty() && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| crate::error("WEBHOOK_SIGNATURE", "Missing or malformed timestamp."))?;
        let seconds = timestamp
            .parse::<i64>()
            .map_err(|_| crate::error("WEBHOOK_SIGNATURE", "Invalid timestamp."))?;
        if request.now.timestamp().abs_diff(seconds) > 300 {
            return Err(crate::error(
                "WEBHOOK_TIMESTAMP",
                "Delivery attempt is outside the five-minute tolerance.",
            ));
        }
        let signature = parts
            .get("v1")
            .filter(|s| s.len() == 64)
            .and_then(|s| hex::decode(s).ok())
            .ok_or_else(|| crate::error("WEBHOOK_SIGNATURE", "Invalid HMAC digest."))?;
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .map_err(|_| crate::error("WEBHOOK_CONFIG", "Invalid HMAC key."))?;
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(request.body);
        mac.verify_slice(&signature)
            .map_err(|_| crate::error("WEBHOOK_SIGNATURE", "Invalid webhook signature."))?;
        // Parsing occurs strictly after raw-body verification.
        #[derive(Deserialize)]
        struct Envelope {
            id: String,
            event: String,
            created_at: DateTime<Utc>,
            data: Value,
        }
        let envelope: Envelope = serde_json::from_slice(request.body)
            .map_err(|_| crate::error("WEBHOOK_PAYLOAD", "Malformed signed webhook envelope."))?;
        if envelope.id.is_empty()
            || envelope.id.len() > 200
            || envelope.id != request.event_id
            || envelope.event != request.event_type
            || !self.binding.allowed_events.contains(&envelope.event)
            || !envelope.data.is_object()
        {
            return Err(crate::error(
                "WEBHOOK_PAYLOAD",
                "Event identity/type does not match the signed envelope and receiver binding.",
            ));
        }
        if envelope
            .data
            .pointer(&self.binding.upstream_id_pointer)
            .and_then(Value::as_str)
            != Some(self.binding.expected_upstream_id.as_str())
        {
            return Err(crate::error(
                "WEBHOOK_SOURCE",
                "Verified event does not belong to the configured upstream source.",
            ));
        }
        Ok(VerifiedWebhook {
            id: envelope.id,
            event: envelope.event,
            created_at: envelope.created_at,
            source_id: self.binding.source_id.clone(),
            data: envelope.data,
            body_hash: crate::hash(request.body),
        })
    }
    pub async fn receive(
        &self,
        request: WebhookRequest<'_>,
        inbox: &dyn WebhookInbox,
    ) -> Result<ReceiptDisposition, SourceError> {
        let event = self.verify(request)?;
        inbox.persist_once(&event).await
    }
}
