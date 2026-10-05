//! Command processor for Telegram C2

use crate::components::shadow_trading::ExecutionEngineType;
use crate::components::telegram_c2::{TelegramCommand, SecurityValidator};
use tracing::{error, info, warn};
use tokio::sync::mpsc;

/// Command processor for Telegram C2
pub struct CommandProcessor {
    security_validator: SecurityValidator,
    risk_manager_tx: Option<mpsc::UnboundedSender<RiskManagementCommand>>,
    execution_engine_tx: Option<mpsc::UnboundedSender<ExecutionEngineCommand>>,
    is_initialized: bool,
}

/// Risk management commands
#[derive(Debug, Clone)]
pub enum RiskManagementCommand {
    SetRiskPercentage(f64),
    HaltSystem,
    ResumeSystem,
    GetStatus,
}

/// Execution engine commands
#[derive(Debug, Clone)]
pub enum ExecutionEngineCommand {
    SetMode(ExecutionEngineType),
    GetStatus,
}

impl CommandProcessor {
    /// Create a new command processor
    pub fn new(authorized_users: Vec<i64>) -> Self {
        Self {
            security_validator: SecurityValidator::new(authorized_users),
            risk_manager_tx: None,
            execution_engine_tx: None,
            is_initialized: false,
        }
    }
    
    /// Initialize the command processor
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.security_validator.initialize()?;
        self.is_initialized = true;
        info!("Command Processor initialized");
        Ok(())
    }
    
    /// Set risk manager communication channel
    pub fn set_risk_manager_channel(&mut self, tx: mpsc::UnboundedSender<RiskManagementCommand>) {
        self.risk_manager_tx = Some(tx);
        info!("Risk manager channel set");
    }
    
    /// Set execution engine communication channel
    pub fn set_execution_engine_channel(&mut self, tx: mpsc::UnboundedSender<ExecutionEngineCommand>) {
        self.execution_engine_tx = Some(tx);
        info!("Execution engine channel set");
    }
    
    /// Process incoming Telegram command
    pub async fn process_command(&mut self, user_id: i64, command: TelegramCommand) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_initialized {
            return Err("Command processor not initialized".into());
        }
        
        // Validate user authorization
        if !self.security_validator.is_user_authorized(user_id) {
            warn!("Unauthorized user {} attempted command", user_id);
            return Ok(());
        }
        
        match command {
            TelegramCommand::Status => {
                self.process_status_command(user_id).await?;
            }
            TelegramCommand::Halt { confirmation_code } => {
                self.process_halt_command(user_id, confirmation_code).await?;
            }
            TelegramCommand::Resume => {
                self.process_resume_command(user_id).await?;
            }
            TelegramCommand::SetRisk { percentage } => {
                self.process_set_risk_command(user_id, percentage).await?;
            }
            TelegramCommand::SetMode { mode, confirmation_code } => {
                self.process_set_mode_command(user_id, mode, confirmation_code).await?;
            }
            TelegramCommand::Help => {
                // Help is handled by the bot itself
            }
        }
        
        Ok(())
    }
    
    /// Process status command
    async fn process_status_command(&self, user_id: i64) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing status command for user {}", user_id);
        
        // Ask both subsystems; either one may answer, the other is not an error.
        self.send_to_risk_manager(RiskManagementCommand::GetStatus, "Status request");
        self.send_to_execution_engine(ExecutionEngineCommand::GetStatus, "Status request");
        
        Ok(())
    }
    
    /// Process halt command
    async fn process_halt_command(&mut self, user_id: i64, confirmation_code: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing halt command for user {}", user_id);
        
        // Validate sensitive command
        if !self.security_validator.validate_sensitive_command(user_id, "halt", confirmation_code) {
            warn!("Halt command rejected for user {} due to security validation", user_id);
            return Ok(());
        }
        
        // Send halt command to risk manager
        self.send_to_risk_manager(RiskManagementCommand::HaltSystem, "Halt command");
        
        Ok(())
    }
    
    /// Process resume command
    async fn process_resume_command(&self, user_id: i64) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing resume command for user {}", user_id);
        
        // Send resume command to risk manager
        self.send_to_risk_manager(RiskManagementCommand::ResumeSystem, "Resume command");
        
        Ok(())
    }
    
    /// Process set risk command
    async fn process_set_risk_command(&self, user_id: i64, percentage: f64) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing set risk command for user {}: {}%", user_id, percentage);
        
        // Validate risk percentage
        if percentage <= 0.0 || percentage > 100.0 {
            warn!("Invalid risk percentage {}% for user {}", percentage, user_id);
            return Ok(());
        }
        
        // Send set risk command to risk manager
        self.send_to_risk_manager(
            RiskManagementCommand::SetRiskPercentage(percentage),
            &format!("Risk percentage update to {:.4}%", percentage),
        );
        
        Ok(())
    }
    
    /// Process set mode command
    async fn process_set_mode_command(
        &mut self,
        user_id: i64,
        mode: ExecutionEngineType,
        confirmation_code: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing set mode command for user {}: {:?}", user_id, mode);
        
        // Arming the live path submits real orders, so it is gated by the same
        // single-use confirmation code as `halt`: the validator already knows the
        // "mode_live" command, this call site just never asked it to check anything.
        if matches!(mode, ExecutionEngineType::Live)
            && !self
                .security_validator
                .validate_sensitive_command(user_id, "mode_live", confirmation_code)
        {
            warn!("Live execution mode rejected for user {} due to security validation", user_id);
            return Ok(());
        }
        
        // Send set mode command to execution engine
        self.send_to_execution_engine(
            ExecutionEngineCommand::SetMode(mode),
            &format!("Execution mode {:?}", mode),
        );
        
        Ok(())
    }
    
    /// Forward a command to the risk manager.
    ///
    /// Failures are reported rather than swallowed: `let _ = tx.send(..)` made an
    /// unwired channel indistinguishable from a delivered `HaltSystem`, i.e. the bot
    /// kept trading while the operator believed it had been stopped.
    fn send_to_risk_manager(&self, command: RiskManagementCommand, description: &str) {
        match &self.risk_manager_tx {
            Some(tx) => {
                if let Err(e) = tx.send(command) {
                    error!("{} was not delivered to the risk manager: {}", description, e);
                } else {
                    info!("{} sent to risk manager", description);
                }
            }
            None => error!("{} dropped: no risk manager channel is wired", description),
        }
    }
    
    /// Forward a command to the execution engine (see [`Self::send_to_risk_manager`]
    /// for why the result is never discarded).
    fn send_to_execution_engine(&self, command: ExecutionEngineCommand, description: &str) {
        match &self.execution_engine_tx {
            Some(tx) => {
                if let Err(e) = tx.send(command) {
                    error!("{} was not delivered to the execution engine: {}", description, e);
                } else {
                    info!("{} sent to execution engine", description);
                }
            }
            None => error!("{} dropped: no execution engine channel is wired", description),
        }
    }
    
    /// Issue a single-use confirmation code for a sensitive command.
    ///
    /// The log line it prints is how the code reaches the engineer; see
    /// `SecurityValidator::issue_confirmation_code` for the lifetime and the reason the
    /// old date-derived code was replaced.
    pub fn issue_confirmation_code(&mut self) -> String {
        self.security_validator.issue_confirmation_code()
    }
    
    /// Check if processor is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}
