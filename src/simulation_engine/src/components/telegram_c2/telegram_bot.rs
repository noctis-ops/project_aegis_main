//! Telegram C2 Bot implementation

use teloxide::{prelude::*, types::ParseMode};
use tracing::{info, debug, warn, error};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Telegram C2 Bot
pub struct TelegramC2Bot {
    bot: Bot,
    authorized_users: Vec<i64>, // Telegram User IDs
    command_tx: mpsc::UnboundedSender<TelegramCommand>,
    notification_rx: mpsc::UnboundedReceiver<TelegramNotification>,
    is_running: bool,
}

/// Telegram command from user
#[derive(Debug, Clone)]
pub enum TelegramCommand {
    Status,
    Halt { confirmation_code: Option<String> },
    Resume,
    SetRisk { percentage: f64 },
    SetMode { mode: crate::components::shadow_trading::ExecutionEngineType },
    Help,
}

/// Telegram notification to send
#[derive(Debug, Clone)]
pub enum TelegramNotification {
    TradeOpened { symbol: String, side: String, price: f64, size: f64 },
    TradeClosed { symbol: String, pnl: f64 },
    CircuitBreakerActivated { level: String, reason: String },
    ConnectionError { error: String },
    SystemStatus { status: String },
}

impl TelegramC2Bot {
    /// Create a new Telegram C2 Bot
    pub fn new(bot_token: String, authorized_users: Vec<i64>) -> Self {
        let bot = Bot::new(bot_token);
        
        let (command_tx, _) = mpsc::unbounded_channel();
        let (_, notification_rx) = mpsc::unbounded_channel();
        
        Self {
            bot,
            authorized_users,
            command_tx,
            notification_rx,
            is_running: false,
        }
    }
    
    /// Initialize the bot with communication channels
    pub fn initialize(
        &mut self, 
        command_tx: mpsc::UnboundedSender<TelegramCommand>,
        notification_rx: mpsc::UnboundedReceiver<TelegramNotification>
    ) {
        self.command_tx = command_tx;
        self.notification_rx = notification_rx;
        info!("Telegram C2 Bot initialized with {} authorized users", self.authorized_users.len());
    }
    
    /// Start the bot
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Ok(());
        }
        
        self.is_running = true;
        info!("Starting Telegram C2 Bot...");
        
        // Clone bot for use in async context
        let bot = self.bot.clone();
        let authorized_users = self.authorized_users.clone();
        let command_tx = self.command_tx.clone();
        
        // Start message handler
        let handler = dptree::entry()
            .branch(Update::filter_message().endpoint(
                move |cx: UpdateWithCx<Message>, bot: Bot| {
                    let authorized_users = authorized_users.clone();
                    let command_tx = command_tx.clone();
                    async move {
                        let user_id = cx.update.from().map(|u| u.id.0).unwrap_or(0);
                        
                        // Check if user is authorized
                        if !authorized_users.contains(&user_id) {
                            bot.send_message(cx.update.chat.id, "❌ غير مصرح لك باستخدام هذا الروبوت")
                                .await?;
                            return Ok(());
                        }
                        
                        // Process command
                        if let Some(text) = cx.update.text() {
                            Self::process_command(&bot, cx.update.chat.id, text, &command_tx).await?;
                        }
                        
                        respond(())
                    }
                }
            ));
        
        // Start dispatcher
        Dispatcher::builder(self.bot.clone(), handler)
            .dependencies(dptree::deps![self.bot.clone()])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
        
        Ok(())
    }
    
    /// Process incoming command
    async fn process_command(
        bot: &Bot, 
        chat_id: ChatId, 
        text: &str, 
        command_tx: &mpsc::UnboundedSender<TelegramCommand>
    ) -> Result<(), Box<dyn std::error::Error>> {
        let command = text.trim();
        
        match command {
            "/start" | "/help" => {
                let help_text = r#"🤖 *Project AEGIS - C2 Control Panel*
الأوامر المتاحة:
/status - عرض حالة النظام
/halt - إيقاف النظام فوراً
/resume - استئناف العمل
/risk <نسبة> - تعديل نسبة المخاطرة
/mode <shadow|live> - تغيير وضع التداول
/help - عرض هذه المساعدة

⚠️ الأوامر الحساسة تتطلب تأكيداً إضافياً"#;
                
                bot.send_message(chat_id, help_text)
                    .parse_mode(ParseMode::Markdown)
                    .await?;
            }
            "/status" => {
                let _ = command_tx.send(TelegramCommand::Status);
                bot.send_message(chat_id, "🔄 جاري جلب حالة النظام...").await?;
            }
            "/halt" => {
                let _ = command_tx.send(TelegramCommand::Halt { confirmation_code: None });
                bot.send_message(chat_id, "⚠️ هل أنت متأكد من إيقاف النظام وإغلاق المراكز؟ أرسل الرمز السري للتأكيد.")
                    .await?;
            }
            "/resume" => {
                let _ = command_tx.send(TelegramCommand::Resume);
                bot.send_message(chat_id, "🔄 جاري استئناف النظام...").await?;
            }
            cmd if cmd.starts_with("/risk ") => {
                if let Ok(percentage) = cmd[6..].trim().parse::<f64>() {
                    let _ = command_tx.send(TelegramCommand::SetRisk { percentage });
                    bot.send_message(chat_id, format!("✅ تم تعديل نسبة المخاطرة إلى {:.4}%", percentage))
                        .await?;
                } else {
                    bot.send_message(chat_id, "❌ قيمة غير صحيحة. استخدم: /risk <نسبة>")
                        .await?;
                }
            }
            cmd if cmd.starts_with("/mode ") => {
                let mode_str = cmd[6..].trim();
                match mode_str {
                    "shadow" => {
                        let _ = command_tx.send(TelegramCommand::SetMode { 
                            mode: crate::components::shadow_trading::ExecutionEngineType::Shadow 
                        });
                        bot.send_message(chat_id, "✅ تم تغيير الوضع إلى التداول الشبحي").await?;
                    }
                    "live" => {
                        let _ = command_tx.send(TelegramCommand::SetMode { 
                            mode: crate::components::shadow_trading::ExecutionEngineType::Live 
                        });
                        bot.send_message(chat_id, "⚠️ هل أنت متأكد من تغيير الوضع إلى التداول الحقيقي؟ أرسل الرمز السري للتأكيد.")
                            .await?;
                    }
                    _ => {
                        bot.send_message(chat_id, "❌ وضع غير صحيح. استخدم: /mode shadow أو /mode live")
                            .await?;
                    }
                }
            }
            _ => {
                bot.send_message(chat_id, "❌ أمر غير معروف. استخدم /help لعرض الأوامر المتاحة")
                    .await?;
            }
        }
        
        Ok(())
    }
    
    /// Send notification to user
    pub async fn send_notification(&self, notification: TelegramNotification) -> Result<(), Box<dyn std::error::Error>> {
        // In a real implementation, this would send the notification
        // For now, we'll just log it
        match &notification {
            TelegramNotification::TradeOpened { symbol, side, price, size } => {
                info!("🔔 Trade opened: {} {} @ {} size={}", symbol, side, price, size);
            }
            TelegramNotification::TradeClosed { symbol, pnl } => {
                info!("🔔 Trade closed: {} PnL={:.4}", symbol, pnl);
            }
            TelegramNotification::CircuitBreakerActivated { level, reason } => {
                warn!("🔔 Circuit breaker activated: {} - {}", level, reason);
            }
            TelegramNotification::ConnectionError { error } => {
                error!("🔔 Connection error: {}", error);
            }
            TelegramNotification::SystemStatus { status } => {
                info!("🔔 System status: {}", status);
            }
        }
        
        Ok(())
    }
    
    /// Check if bot is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}