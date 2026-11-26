//! Web Push: browsers subscribe, and each new notification is pushed to
//! them while "outside the app" is on, so it reaches a closed tab or a
//! locked phone.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::modules::clients::ClientService;
use crate::modules::notifications::notification::model::Notification;
use crate::modules::notifications::notification::service::{
    NotificationEvent, NotificationService,
};
use crate::modules::notifications::push::model::{
    self, Delivery, NewSubscription, Subscription, VapidKey,
};
use crate::modules::notifications::push::store::PushRepository;
use crate::shared::error::ServiceError;
use crate::shared::time::Clock;

/// How long a push service keeps a message for a browser that is away.
const KEEP_FOR: Duration = Duration::from_hours(24);

pub struct PushService {
    repo: Arc<dyn PushRepository>,
    clients: Arc<ClientService>,
    notifications: Arc<NotificationService>,
    clock: Arc<dyn Clock>,
    /// The kept key; until `use_key`, one made on first use (tests).
    key: RwLock<Option<Arc<VapidKey>>>,
    http: reqwest::Client,
}

fn not_found() -> ServiceError {
    ServiceError::NotFound {
        code: "push_subscription_not_found",
        message: "No such push subscription.".to_owned(),
    }
}

fn internal(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Internal(error.to_string())
}

impl PushService {
    pub fn new(
        repo: Arc<dyn PushRepository>,
        clients: Arc<ClientService>,
        notifications: Arc<NotificationService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            clients,
            notifications,
            clock,
            key: RwLock::new(None),
            http: reqwest::Client::new(),
        }
    }

    /// The key kept with the server's other keys.
    pub fn use_key(&self, key: VapidKey) {
        *self
            .key
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(key));
    }

    fn key(&self) -> Result<Arc<VapidKey>, ServiceError> {
        let mut key = self
            .key
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(key) = key.as_ref() {
            return Ok(Arc::clone(key));
        }
        let made = Arc::new(VapidKey::generate().map_err(internal)?);
        *key = Some(Arc::clone(&made));
        Ok(made)
    }

    /// What browsers subscribe with (`applicationServerKey`).
    pub fn public_key(&self) -> Result<String, ServiceError> {
        Ok(self.key()?.public())
    }

    pub async fn subscribe(
        &self,
        client: Uuid,
        new: NewSubscription,
    ) -> Result<Subscription, ServiceError> {
        // A revoked client cannot subscribe; this also checks it exists.
        self.clients.get(client).await?;
        let subscription = Subscription {
            id: Uuid::now_v7(),
            client_id: client,
            endpoint: new.endpoint,
            p256dh: new.p256dh,
            auth: new.auth,
            label: new.label,
            created_at: self.clock.now(),
            last_used_at: None,
        };
        self.repo.add(&subscription).await?;
        tracing::info!(subscription = %subscription.id, %client, "a browser subscribed to pushes");
        Ok(subscription)
    }

    /// Every subscription of a client still paired.
    pub async fn list(&self) -> Result<Vec<Subscription>, ServiceError> {
        let mut kept = Vec::new();
        for subscription in self.repo.list().await? {
            if self.clients.get(subscription.client_id).await.is_ok() {
                kept.push(subscription);
            }
        }
        Ok(kept)
    }

    pub async fn unsubscribe(&self, id: Uuid) -> Result<(), ServiceError> {
        if !self.repo.remove(id).await? {
            return Err(not_found());
        }
        Ok(())
    }

    /// Pushes a test message to one subscription; whether it arrived at
    /// its push service.
    pub async fn test(&self, id: Uuid) -> Result<Delivery, ServiceError> {
        let subscription = self.repo.get(id).await?.ok_or_else(not_found)?;
        Ok(self
            .push(&subscription, &model::test_payload(), "high")
            .await)
    }

    /// Pushes `notification` to every subscription, when the settings let
    /// it go outside the app.
    pub async fn deliver(&self, notification: &Notification) -> Result<(), ServiceError> {
        let settings = self.notifications.settings().await?;
        if !settings.goes_outside(&notification.kind) {
            return Ok(());
        }
        let payload = model::payload(notification);
        let urgency = match notification.level {
            crate::modules::notifications::notification::model::Level::Warning
            | crate::modules::notifications::notification::model::Level::Error => "high",
            _ => "normal",
        };
        for subscription in self.repo.list().await? {
            if self.clients.get(subscription.client_id).await.is_err() {
                // Revoked: it hears nothing more.
                self.repo.remove(subscription.id).await?;
                continue;
            }
            self.push(&subscription, &payload, urgency).await;
        }
        Ok(())
    }

    async fn push(&self, subscription: &Subscription, payload: &[u8], urgency: &str) -> Delivery {
        let delivery = match self.try_push(subscription, payload, urgency).await {
            Ok(status) => model::delivery(status),
            Err(error) => {
                tracing::info!(subscription = %subscription.id, %error, "cannot push");
                Delivery::Failed
            }
        };
        let kept = match delivery {
            Delivery::Sent => self.repo.used(subscription.id, self.clock.now()).await,
            Delivery::Gone => {
                tracing::info!(subscription = %subscription.id, "the browser unsubscribed");
                self.repo.remove(subscription.id).await.map(|_| ())
            }
            Delivery::Failed => Ok(()),
        };
        if let Err(error) = kept {
            tracing::warn!(subscription = %subscription.id, %error, "cannot keep a push's outcome");
        }
        delivery
    }

    async fn try_push(
        &self,
        subscription: &Subscription,
        payload: &[u8],
        urgency: &str,
    ) -> Result<u16, ServiceError> {
        let body =
            model::encrypt(&subscription.p256dh, &subscription.auth, payload).map_err(internal)?;
        let authorization = self
            .key()?
            .authorization(&subscription.endpoint, self.clock.now())
            .map_err(internal)?;
        let answer = self
            .http
            .post(&subscription.endpoint)
            .header("Authorization", authorization)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("TTL", KEEP_FOR.as_secs().to_string())
            .header("Urgency", urgency)
            .timeout(Duration::from_secs(15))
            .body(body)
            .send()
            .await
            .map_err(internal)?;
        let status = answer.status().as_u16();
        if !answer.status().is_success() {
            let reason = answer.text().await.unwrap_or_default();
            tracing::info!(subscription = %subscription.id, status, reason = %reason.chars().take(200).collect::<String>(), "the push service refused a push");
        }
        Ok(status)
    }

    /// Pushes every new notification, for as long as the server runs.
    pub fn spawn(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let mut events = self.notifications.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if let NotificationEvent::Created(notification) = event.as_ref()
                            && let Err(error) = service.deliver(notification).await
                        {
                            tracing::warn!(%error, "cannot push a notification");
                        }
                    }
                    Err(RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "pushes fell behind; some were not pushed");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}
