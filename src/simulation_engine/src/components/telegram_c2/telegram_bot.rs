//! Telegram C2 Bot implementation

use teloxide::{prelude::*, types::ParseMode};
use tracing::{debug, error, info, warn};
use tokio::sync::mpsc;
use crate::components::shadow_trading::ExecutionEngineType;

/// Telegram C2 Bot
pub struct TelegramC2Bot {
    bot: Bot,
    authorized_users: Vec<i64>, // Telegram User IDs
    command_tx: mpsc::UnboundedSender<TelegramCommand>,
    is_running: bool,
}

/// Telegram command from user
#[derive(Debug, Clone)]
pub enum TelegramCommand {
    Status,
    Halt { confirmation_code: Option<String> },
    Resume,
    SetRisk { percentage: f64 },
    SetMode {
        mode: ExecutionEngineType,
        confirmation_code: Option<String>,
    },
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

/// A command parsed out of a message, plus what the operator is told about it.
///
/// Grouping the three because the text must follow the command: promising
/// "adjusting the risk limit..." after a delivery that failed is how a control
/// channel makes an operator believe something happened that did not.
struct ParsedCommand {
    command: TelegramCommand,
    label: &'static str,
    ack: String,
}

impl TelegramC2Bot {
    /// Create the bot, together with the receiving end of its command channel.
    ///
    /// The receiver has to be handed to the [`CommandProcessor`](super::CommandProcessor)
    /// that consumes commands. It used to be created here and dropped on the floor
    /// (`let (command_tx, _) = unbounded_channel()`), so every `/status`, `/halt`,
    /// `/risk` and `/mode` send failed against a closed channel and was discarded by
    /// a `let _ =` - the C2 panel could not reach the engine at all, silently.
    pub fn new(
        bot_token: String,
        authorized_users: Vec<i64>,
    ) -> (Self, mpsc::UnboundedReceiver<TelegramCommand>) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let bot = Self {
            bot: Bot::new(bot_token),
            authorized_users,
            command_tx,
            is_running: false,
        };

        info!("Telegram C2 Bot created with {} authorized users", bot.authorized_users.len());
        (bot, command_rx)
    }
    
    /// Start the bot: poll Telegram until ctrl-c.
    ///
    /// `dispatch()` runs forever, so this belongs in a `tokio::spawn` rather than on
    /// a startup path that has other work to do.
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running {
            return Ok(());
        }
        
        self.is_running = true;
        info!("Starting Telegram C2 Bot...");
        
        // teloxide 0.12 injects the filtered update payload into the endpoint
        // arguments (there is no `UpdateWithCx` wrapper any more); the bot handle and
        // the C2 wiring are captured here rather than passed through `dptree::deps`.
        let bot = self.bot.clone();
        let authorized_users = self.authorized_users.clone();
        let command_tx = self.command_tx.clone();
        
        let handler = dptree::entry()
            .branch(Update::filter_message().endpoint(move |msg: Message| {
                let bot = bot.clone();
                let authorized_users = authorized_users.clone();
                let command_tx = command_tx.clone();
                async move { Self::handle_message(bot, msg, authorized_users, command_tx).await }
            }));
        
        Dispatcher::builder(self.bot.clone(), handler)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
        
        Ok(())
    }
    
    /// Handle one inbound message: authorize the sender, then parse the command.
    async fn handle_message(
        bot: Bot,
        msg: Message,
        authorized_users: Vec<i64>,
        command_tx: mpsc::UnboundedSender<TelegramCommand>,
    ) -> Result<(), teloxide::RequestError> {
        let chat_id = msg.chat.id;

        // A message without a sender (a channel post, say) cannot be attributed to
        // an operator, so it is refused; the previous `unwrap_or(0)` handed such
        // messages the id `0` and let the authorization check decide on a guess.
        let user_id = match msg.from() {
            Some(user) => user.id.0 as i64, // `UserId` is u64; the allow-list is i64 end to end
            None => {
                warn!("Telegram message in chat {} has no sender; ignoring", chat_id.0);
                return Ok(());
            }
        };

        if !authorized_users.contains(&user_id) {
            warn!("Unauthorized Telegram user {} attempted a C2 command", user_id);
            bot.send_message(chat_id, "❌ غير مصرح لك باستخدام هذا الروبوت").await?;
            return Ok(());
        }

        let text = match msg.text() {
            Some(text) => text,
            None => return Ok(()), // not a text message: nothing to parse
        };

        debug!("Telegram C2 command '{}' from user {}", text, user_id);

        // `process_command` reports Telegram API and channel failures in its own
        // error type, which is not `RequestError`; the operator has already been
        // answered by it, so the failure is logged rather than `?`-ed into a handler
        // that cannot produce that error.
        if let Err(e) = Self::process_command(&bot, chat_id, text, &command_tx).await {
            error!("Telegram C2 command '{}' failed: {}", text, e);
        }

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

        // The help card is composed here rather than forwarded: the bot answers it
        // itself, and `CommandProcessor` treats `Help` as a no-op for the same reason.
        if command == "/start" || command == "/help" {
            bot.send_message(chat_id, help_text())
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
            return Ok(());
        }

        let parsed = match Self::parse_command(command) {
            Ok(parsed) => parsed,
            Err(reply) => {
                bot.send_message(chat_id, reply).await?;
                return Ok(());
            }
        };

        let reply = match forward_command(command_tx, parsed.command, parsed.label) {
            Ok(()) => parsed.ack,
            Err(e) => format!("⚠️ لم يتم تمرير {} إلى محرك التداول: {}", parsed.label, e),
        };

        bot.send_message(chat_id, reply).await?;
        Ok(())
    }

    /// Turn message text into a C2 command and the confirmation it earns.
    fn parse_command(text: &str) -> Result<ParsedCommand, String> {
        if let Some(arg) = text.strip_prefix("/risk ") {
            return match arg.trim().parse::<f64>() {
                Ok(percentage) => Ok(ParsedCommand {
                    command: TelegramCommand::SetRisk { percentage },
                    label: "/risk",
                    ack: format!("✅ تم تعديل نسبة المخاطرة إلى {:.4}%", percentage),
                }),
                Err(_) => Err("❌ قيمة غير صحيحة. استخدم: /risk <نسبة>".to_string()),
            };
        }

        if let Some(rest) = text.strip_prefix("/halt").filter(|rest| rest.is_empty() || rest.starts_with(' ')) {
            // The message the bot sends asks for a confirmation code, so the command
            // has to carry one: without it the processor's `SecurityValidator` refuses
            // the halt every single time, i.e. the emergency stop was unreachable.
            let code = rest.trim();
            return Ok(ParsedCommand {
                command: TelegramCommand::Halt {
                    confirmation_code: (!code.is_empty()).then(|| code.to_string()),
                },
                label: "/halt",
                ack: if code.is_empty() {
                    "⚠️ الإيقاف يتطلب رمز التأكيد: /halt <الرمز السري> - هل أنت متأكد من إيقاف النظام وإغلاق المراكز؟".to_string()
                } else {
                    "🛑 جاري تنفيذ أمر الإيقاف بعد التحقق من الرمز...".to_string()
                },
            });
        }

        if let Some(rest) = text.strip_prefix("/mode ") {
            let mut words = rest.split_whitespace();
            let mode = match words.next().unwrap_or("") {
                "shadow" => ExecutionEngineType::Shadow,
                "live" => ExecutionEngineType::Live,
                _ => return Err("❌ وضع غير صحيح. استخدم: /mode shadow أو /mode live".to_string()),
            };
            let confirmation_code = words.next().map(|word| word.to_string());

            // Arming the live path submits real orders, so it is the one command
            // that is answered with a confirmation request instead of a receipt.
            let ack = match (mode, &confirmation_code) {
                (ExecutionEngineType::Shadow, _) => "✅ تم تغيير الوضع إلى التداول الشبحي",
                (ExecutionEngineType::Live, None) => {
                    "⚠️ التداول الحقيقي يتطلب رمز التأكيد: /mode live <الرمز السري>"
                }
                (ExecutionEngineType::Live, Some(_)) => {
                    "🛑 جاري التبديل إلى التداول الحقيقي بعد التحقق من الرمز..."
                }
            };

            return Ok(ParsedCommand {
                command: TelegramCommand::SetMode { mode, confirmation_code },
                label: "/mode",
                ack: ack.to_string(),
            });
        }

        match text {
            "/status" => Ok(ParsedCommand {
                command: TelegramCommand::Status,
                label: "/status",
                ack: "🔄 جاري جلب حالة النظام...".to_string(),
            }),
            "/resume" => Ok(ParsedCommand {
                command: TelegramCommand::Resume,
                label: "/resume",
                ack: "🔄 جاري استئناف النظام...".to_string(),
            }),
            _ => Err("❌ أمر غير معروف. استخدم /help لعرض الأوامر المتاحة".to_string()),
        }
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

/// Hand a command to the processor, reporting a closed channel instead of
/// discarding it: `let _ = command_tx.send(..)` made an unwired C2 path look
/// exactly like a command that had been accepted.
fn forward_command(
    command_tx: &mpsc::UnboundedSender<TelegramCommand>,
    command: TelegramCommand,
    label: &str,
) -> Result<(), String> {
    command_tx.send(command).map_err(|e| {
        let reason = format!("{}", e);
        error!("Telegram {} was not delivered to the command processor: {}", label, reason);
        reason
    })
}

/// The `/help` payload, escaped for Telegram's MarkdownV2 parser.
///
/// MarkdownV2 replaced the legacy Markdown mode, which teloxide 0.12 deprecates;
/// the difference is not cosmetic - MarkdownV2 rejects (HTTP 400) any unescaped
/// `_*[]()~`>#+-=|{}.!`, and this text is full of `-`, `.` and `|`.
fn help_text() -> String {
    let lines: &[(&str, &str)] = &[
        ("/status", "عرض حالة النظام"),
        ("/halt <الرمز>", "إيقاف النظام فوراً"),
        ("/resume", "استئناف العمل"),
        ("/risk <نسبة>", "تعديل نسبة المخاطرة"),
        ("/mode <shadow|live> [الرمز]", "تغيير وضع التداول"),
        ("/help", "عرض هذه المساعدة"),
    ];

    let mut text = format!(
        "🤖 *{}*\n{}\n",
        escape_markdown_v2("Project AEGIS - C2 Control Panel"),
        escape_markdown_v2("الأوامر المتاحة:")
    );

    for (command, description) in lines {
        // The whole line is escaped in one pass: the ` - ` separator contains a `-`,
        // which MarkdownV2 treats as syntax just like the characters inside the text.
        text.push_str(&escape_markdown_v2(&format!("{} - {}", command, description)));
        text.push('\n');
    }

    text.push_str(&escape_markdown_v2("⚠️ الأوامر الحساسة تتطلب تأكيداً إضافياً"));
    text
}

/// Escape the characters Telegram's MarkdownV2 parser treats as syntax.
fn escape_markdown_v2(text: &str) -> String {
    const SPECIAL: [char; 18] = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];

    let mut escaped = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if SPECIAL.contains(&c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_v2_escapes_every_reserved_character() {
        assert_eq!(
            escape_markdown_v2("a*b_c[d]e(f)g~h`i>j#k+l-m=n|o{p}q.r!s"),
            "a\\*b\\_c\\[d\\]e\\(f\\)g\\~h\\`i\\>j\\#k\\+l\\-m\\=n\\|o\\{p\\}q\\.r\\!s"
        );
    }

    #[test]
    fn help_text_leaves_only_the_header_emphasis_unescaped() {
        let text = help_text();

        assert!(text.starts_with("🤖 *Project AEGIS \\- C2 Control Panel*\n"));
        assert!(text.contains("/status \\- عرض حالة النظام"));
        // Unbalanced emphasis markers make Telegram reject the message as
        // malformed, and the header is the only place that may carry one.
        assert_eq!(text.matches('*').count(), 2);
    }

    #[test]
    fn forwarded_commands_reach_the_receiver_and_closed_channels_are_reported() {
        let (tx, mut rx) = mpsc::unbounded_channel();

        forward_command(&tx, TelegramCommand::Status, "/status").expect("open channel");
        assert!(matches!(rx.try_recv(), Ok(TelegramCommand::Status)));

        drop(rx);
        assert!(forward_command(&tx, TelegramCommand::Status, "/status").is_err());
    }

    #[test]
    fn bot_creation_hands_over_a_live_command_receiver() {
        // Regression: `new()` used to drop the receiver, so no command could ever be
        // delivered and the failure left no trace.
        let (bot, mut command_rx) = TelegramC2Bot::new("123456:TESTTOKEN".to_string(), vec![42]);

        assert!(!bot.is_running());
        assert_eq!(bot.authorized_users, vec![42]);
        assert!(bot.command_tx.send(TelegramCommand::Help).is_ok());
        assert!(matches!(command_rx.try_recv(), Ok(TelegramCommand::Help)));
    }

    #[test]
    fn risk_and_mode_arguments_are_parsed_or_rejected() {
        let risk = TelegramC2Bot::parse_command("/risk 0.75").expect("valid risk percentage");
        assert!(matches!(risk.command, TelegramCommand::SetRisk { percentage } if (percentage - 0.75).abs() < f64::EPSILON));
        assert_eq!(risk.label, "/risk");

        let shadow = TelegramC2Bot::parse_command("/mode shadow").expect("known mode");
        assert!(matches!(
            shadow.command,
            TelegramCommand::SetMode { mode: ExecutionEngineType::Shadow, .. }
        ));

        let live = TelegramC2Bot::parse_command("/mode live").expect("known mode");
        assert!(matches!(live.command, TelegramCommand::SetMode { mode: ExecutionEngineType::Live, .. }));
        // Arming live mode must ask for confirmation, not report success.
        assert!(live.ack.starts_with("⚠️"));

        let armed = TelegramC2Bot::parse_command("/mode live AEGIS20261005").expect("mode with code");
        assert!(matches!(
            armed.command,
            TelegramCommand::SetMode { confirmation_code: Some(_), .. }
        ));

        // A halt without its code is forwarded too (so the attempt is recorded), but
        // the reply tells the operator it will not be accepted.
        let halt = TelegramC2Bot::parse_command("/halt").expect("halt parses");
        assert!(matches!(halt.command, TelegramCommand::Halt { confirmation_code: None }));
        let confirmed = TelegramC2Bot::parse_command("/halt AEGIS20261005").expect("halt with code");
        assert!(matches!(
            confirmed.command,
            TelegramCommand::Halt { confirmation_code: Some(_) }
        ));

        assert!(TelegramC2Bot::parse_command("/risk abc").is_err());
        assert!(TelegramC2Bot::parse_command("/mode yolo").is_err());
        assert!(TelegramC2Bot::parse_command("/frobnicate").is_err());
    }
}
