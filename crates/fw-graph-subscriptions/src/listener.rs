//! Postgres LISTEN-based subscription manager.
//!
//! `PgSubscriptionManager` maintains a dedicated `PgListener` connection,
//! multiplexes incoming NOTIFY messages onto per-channel `broadcast` channels,
//! and automatically reconnects on connection failure.

use std::sync::Arc;

use dashmap::DashMap;
use sqlx::postgres::{PgListener, PgPool};
use tokio::sync::broadcast;
use tracing::error;

use crate::error::SubscriptionError;
use crate::publisher::NotifyPayload;

/// Broadcast capacity per channel.
pub const BROADCAST_CAPACITY: usize = 256;

/// Manages Postgres LISTEN subscriptions and fans out NOTIFY messages to
/// Tokio broadcast receivers.
pub struct PgSubscriptionManager {
    listener: PgListener,
    channels: Arc<DashMap<String, broadcast::Sender<NotifyPayload>>>,
    /// Retained so callers can spawn additional listeners or acquire connections
    /// without passing the pool separately.
    #[allow(dead_code)]
    pool: PgPool,
}

impl PgSubscriptionManager {
    /// Create a new manager using a dedicated LISTEN connection from `pool`.
    pub async fn new(pool: PgPool) -> Result<Self, SubscriptionError> {
        let listener = PgListener::connect_with(&pool).await?;
        Ok(Self {
            listener,
            channels: Arc::new(DashMap::new()),
            pool,
        })
    }

    /// Start listening on `channel` (if not already) and return a receiver.
    ///
    /// Multiple callers can subscribe to the same channel; each gets an
    /// independent broadcast receiver.
    pub async fn subscribe(
        &mut self,
        channel: &str,
    ) -> Result<broadcast::Receiver<NotifyPayload>, SubscriptionError> {
        // If a sender already exists, just return a new receiver.
        if let Some(sender) = self.channels.get(channel) {
            return Ok(sender.subscribe());
        }

        // New channel: issue LISTEN and create the broadcast sender.
        self.listener.listen(channel).await?;
        let (tx, rx) = broadcast::channel(BROADCAST_CAPACITY);
        self.channels.insert(channel.to_string(), tx);
        Ok(rx)
    }

    /// Stop listening on `channel` and remove its broadcast sender.
    pub async fn unsubscribe(&mut self, channel: &str) -> Result<(), SubscriptionError> {
        self.listener.unlisten(channel).await?;
        self.channels.remove(channel);
        Ok(())
    }

    /// Return a clone of the shared channels map (for inspection / testing).
    pub fn channels(&self) -> Arc<DashMap<String, broadcast::Sender<NotifyPayload>>> {
        Arc::clone(&self.channels)
    }

    /// Number of active LISTEN channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Main dispatch loop. Call this in a spawned task:
    ///
    /// ```ignore
    /// tokio::spawn(manager.run());
    /// ```
    ///
    /// The loop:
    /// 1. Waits for the next NOTIFY message from Postgres.
    /// 2. Looks up the matching broadcast sender.
    /// 3. Sends a [`NotifyPayload`] to all receivers (lagged receivers are
    ///    dropped silently — `broadcast::error::SendError` means no receivers).
    /// 4. On connection error, logs the error and sleeps 1 s before retrying
    ///    (`PgListener` auto-reconnects on the next `recv()` call).
    pub async fn run(mut self) {
        loop {
            match self.listener.recv().await {
                Ok(notification) => {
                    let channel = notification.channel().to_string();
                    let pk_text = notification.payload().to_string();

                    let payload = NotifyPayload { channel: channel.clone(), pk_text };

                    if let Some(sender) = self.channels.get(&channel) {
                        // Ignore send errors — they just mean no receivers are active.
                        let _ = sender.send(payload);
                    }
                }
                Err(e) => {
                    error!(error = %e, "PgSubscriptionManager: recv error — will retry");
                    // PgListener reconnects automatically; brief pause avoids a
                    // tight retry loop if Postgres is temporarily unavailable.
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publisher::NotifyPayload;

    // Test 8: NotifyPayload is Clone
    #[test]
    fn notify_payload_is_clone() {
        let p = NotifyPayload {
            channel: "public_concepts_mutation".to_string(),
            pk_text: "42".to_string(),
        };
        let cloned = p.clone();
        assert_eq!(cloned.channel, p.channel);
        assert_eq!(cloned.pk_text, p.pk_text);
    }

    // Test 9: broadcast capacity constant
    #[test]
    fn broadcast_capacity_is_256() {
        assert_eq!(BROADCAST_CAPACITY, 256);
    }
}
