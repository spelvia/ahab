//! The Web Push API's types.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::notifications::push::model::{Delivery, Subscription};

/// The server's VAPID public key, for `PushManager.subscribe`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PushKey)]
pub struct PushKeyDto {
    /// Base64url, unpadded: the `applicationServerKey`.
    pub public_key: String,
}

/// A subscription's keys, as `PushSubscription.toJSON()` gives them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PushSubscriptionKeys)]
pub struct KeysDto {
    pub p256dh: String,
    pub auth: String,
}

/// A browser subscribing: what `PushSubscription.toJSON()` gives, and a
/// label for Settings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreatePushSubscription)]
pub struct CreateSubscriptionDto {
    pub endpoint: String,
    pub keys: KeysDto,
    /// "Firefox on Linux".
    #[serde(default)]
    pub label: String,
}

/// A browser that gets pushes.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PushSubscription)]
pub struct SubscriptionDto {
    pub id: Uuid,
    /// The paired client that registered it.
    pub client_id: Uuid,
    pub label: String,
    /// Where its pushes go, such as `fcm.googleapis.com`.
    pub service: String,
    /// The endpoint, so a browser can find its own.
    pub endpoint: String,
    /// Whether the client asking registered it.
    pub this_client: bool,
    #[serde(with = "crate::shared::time::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
}

impl SubscriptionDto {
    pub fn new(subscription: Subscription, caller: Option<Uuid>) -> Self {
        let service = reqwest::Url::parse(&subscription.endpoint)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        Self {
            id: subscription.id,
            client_id: subscription.client_id,
            label: subscription.label,
            service,
            endpoint: subscription.endpoint,
            this_client: caller == Some(subscription.client_id),
            created_at: subscription.created_at,
            last_used_at: subscription.last_used_at,
        }
    }
}

/// What a test push came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PushOutcome)]
pub enum OutcomeDto {
    /// The push service took it; the browser shows it when it can.
    Sent,
    /// The browser had unsubscribed; the subscription is removed.
    Gone,
    /// The push service could not be reached or refused it.
    Failed,
}

impl From<Delivery> for OutcomeDto {
    fn from(delivery: Delivery) -> Self {
        match delivery {
            Delivery::Sent => Self::Sent,
            Delivery::Gone => Self::Gone,
            Delivery::Failed => Self::Failed,
        }
    }
}

/// A test push's outcome.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = PushTest)]
pub struct TestDto {
    pub outcome: OutcomeDto,
}
