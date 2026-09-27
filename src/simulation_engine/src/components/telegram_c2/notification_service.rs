//! Notification service for Telegram C2

use crate::components::telegram_c2::TelegramNotification;
use tracing::{info, debug, warn, error};
use tokio::sync::mpsc;
use std::sync::Arc;

/// Notification service for Telegram C2
pub struct NotificationService {
    notification_rx: Option<mpsc::UnboundedReceiver<TelegramNotification>>,
    telegram_bot: Option<Arc<tokio::sync::Mutex<crate::components::telegram_c2::TelegramC2Bot>>>,
    retry_queue: Vec<TelegramNotification>,
    max_retry_attempts: usize,
    is_running: bool,
}

impl NotificationService {
    /// Create a new notification service
    pub fn new(max_retry_attempts: usize) -> Self {
        Self {
            notification_rx: None,
            telegram_bot: None,
            retry_queue: Vec::new(),
            max_retry_attempts,
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
    
    /// Start the notification service
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Ok(());
        }
        
        self.is_running = true;
        info!("Starting Notification Service...");
        
        // Start notification processing loop
        self.process_notifications().await?;
        
        Ok(())
    }
    
    /// Process notifications in a loop
    async fn process_notifications(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut rx) = self.notification_rx {
            while let Some(notification) = rx.recv().await {
                if let Err(e) = self.send_notification(notification.clone()).await {
                    error!("Failed to send notification: {}. Adding to retry queue.", e);
                    self.retry_queue.push(notification);
                }
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
            // In a real implementation, we would send to Telegram
            // For now, we'll just log the notification
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
    
    /// Process retry queue
    async fn process_retry_queue(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut failed_notifications = Vec::new();
        
        for notification in self.retry_queue.drain(..) {
            if let Err(e) = self.send_notification(notification.clone()).await {
                error!("Retry failed for notification: {}", e);
                failed_notifications.push(notification);
            }
        }
        
        // Keep failed notifications for next retry (up to max attempts)
        self.retry_queue = failed_notifications;
        
        Ok(())
    }
    
    /// Add notification to queue (fire-and-forget)
    pub fn queue_notification(&self, notification: TelegramNotification) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref rx) = self.notification_rx {
            // In a real implementation, we would send to the channel
            // For now, we'll just log that it was queued
            info!("Notification queued: {:?}", notification);
            Ok(())
        } else {
            Err("Notification receiver not set".into())
        }
    }
    
    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}