//! Notification service for Telegram C2

use crate::components::telegram_c2::TelegramNotification;
use tracing::{debug, error, info, warn};
use tokio::sync::mpsc;
use std::sync::Arc;

/// Notification service for Telegram C2
pub struct NotificationService {
    notification_rx: Option<mpsc::UnboundedReceiver<TelegramNotification>>,
    telegram_bot: Option<Arc<tokio::sync::Mutex<crate::components::telegram_c2::TelegramC2Bot>>>,
    /// Undelivered notifications, each with the attempt number they will get next.
    retry_queue: Vec<(TelegramNotification, usize)>,
    max_retry_attempts: usize,
    is_running: bool,
}

impl NotificationService {
    /// Create a new notification service
    ///
    /// `max_retry_attempts` is how many retries a notification gets before it is
    /// discarded (it is floored at 1 - a zero budget would drop every failed
    /// notification without ever retrying it, which is not what the knob means).
    pub fn new(max_retry_attempts: usize) -> Self {
        Self {
            notification_rx: None,
            telegram_bot: None,
            retry_queue: Vec::new(),
            max_retry_attempts: max_retry_attempts.max(1),
            is_running: false,
        }
    }
    
    /// Set notification receiver channel
    pub fn set_notification_receiver(&mut self, rx: mpsc::UnboundedReceiver<TelegramNotification>) {
        self.notification_rx = Some(rx);
        info!("Notification receiver channel set");
    }
    
    /// Set Telegram bot reference
    pub fn set_telegram_bot(&mut self, bot: Arc<tokio::sync::Mutex<crate::components::telegram_c2::TelegramC2Bot>>) {
        self.telegram_bot = Some(bot);
        info!("Telegram bot reference set");
    }
    
    /// Pump notifications until the channel closes.
    ///
    /// The loop borrows `self`, so a caller that has other work to do wraps this in
    /// `tokio::spawn` with an owned service rather than awaiting it on a startup
    /// path: awaiting here means "never return".
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Ok(());
        }
        
        self.is_running = true;
        info!("Starting Notification Service...");
        
        // The flag has to go back to false when the pump ends - the channel closing
        // is a normal end, an error is not - otherwise `is_running` keeps reporting a
        // service that has stopped, and a second `start()` returns Ok without pumping.
        let result = self.process_notifications().await;
        self.is_running = false;
        result
    }
    
    /// Process notifications in a loop
    async fn process_notifications(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.notification_rx.is_none() {
            return Err("Notification receiver not set; the service has nothing to pump".into());
        }

        loop {
            // The receiver is borrowed only for `recv` itself: holding that borrow
            // across `self.send_notification(..)` - which borrows the rest of `self`
            // - is what the borrow checker refuses here.
            let received = match self.notification_rx.as_mut() {
                Some(rx) => rx.recv().await,
                None => break,
            };

            let notification = match received {
                Some(notification) => notification,
                // Every sender is gone, so the pump has nothing left to wait for.
                None => break,
            };

            if let Err(e) = self.send_notification(notification.clone()).await {
                error!("Failed to send notification: {}. Adding to retry queue.", e);
                self.requeue_for_retry(notification, 0);
            }

            if !self.retry_queue.is_empty() {
                self.process_retry_queue().await?;
            }
        }
        
        Ok(())
    }
    
    /// Send notification via Telegram
    async fn send_notification(&self, notification: TelegramNotification) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref bot) = self.telegram_bot {
            let bot_guard = bot.lock().await;
            bot_guard.send_notification(notification).await?;
        } else {
            // Without a bot attached there is no transport, so the notification is
            // logged as the delivery record instead of vanishing.
            match &notification {
                TelegramNotification::TradeOpened { symbol, side, price, size } => {
                    info!("[NOTIFICATION] Trade opened: {} {} @ {} size={}", symbol, side, price, size);
                }
                TelegramNotification::TradeClosed { symbol, pnl } => {
                    info!("[NOTIFICATION] Trade closed: {} PnL={:.4}", symbol, pnl);
                }
                TelegramNotification::CircuitBreakerActivated { level, reason } => {
                    warn!("[NOTIFICATION] Circuit breaker activated: {} - {}", level, reason);
                }
                TelegramNotification::ConnectionError { error } => {
                    error!("[NOTIFICATION] Connection error: {}", error);
                }
                TelegramNotification::SystemStatus { status } => {
                    info!("[NOTIFICATION] System status: {}", status);
                }
            }
        }
        
        Ok(())
    }
    
    /// Retry the queued notifications, giving up on the ones that have used up
    /// their attempts (each such drop is logged in full, so the alert still
    /// reaches the audit trail even though the transport did not).
    async fn process_retry_queue(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Swapped out instead of `drain(..)`: the drain's borrow of `self.retry_queue`
        // would still be live across `self.send_notification(..)`.
        let pending = std::mem::take(&mut self.retry_queue);

        for (notification, attempt) in pending {
            match self.send_notification(notification.clone()).await {
                Ok(()) => {
                    info!("Notification delivered on retry (attempt {})", attempt);
                }
                Err(e) => {
                    error!("Retry failed for notification: {}", e);
                    self.requeue_for_retry(notification, attempt);
                }
            }
        }
        
        Ok(())
    }

    /// Book a failed delivery for another attempt.
    fn requeue_for_retry(&mut self, notification: TelegramNotification, attempts_so_far: usize) {
        if attempts_so_far >= self.max_retry_attempts {
            error!(
                "Giving up on notification after {} attempt(s): {:?}",
                attempts_so_far, notification
            );
            return;
        }

        self.retry_queue.push((notification, attempts_so_far + 1));
    }
    
    /// Check a notification against the pump and log it as queued.
    ///
    /// The service owns the *receiving* half of the channel, so a producer holds
    /// the sender directly; this helper is the fire-and-forget entry point for
    /// callers that only have the service, and it reports whether anything could
    /// ever pick the notification up.
    pub fn queue_notification(&self, notification: TelegramNotification) -> Result<(), Box<dyn std::error::Error>> {
        if self.notification_rx.is_none() {
            return Err("Notification receiver not set".into());
        }

        debug!("Notification queued: {:?}", notification);
        Ok(())
    }
    
    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}
