//! Command processor for Telegram C2

use crate::components::telegram_c2::{TelegramCommand, SecurityValidator};
use tracing::{info, debug, warn, error};
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
    SetMode(crate::components::shadow_trading::ExecutionEngineType),
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
            TelegramCommand::SetMode { mode } => {
                self.process_set_mode_command(user_id, mode).await?;
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
        
        // Send status requests to both risk manager and execution engine
        if let Some(tx) = &self.risk_manager_tx {
            let _ = tx.send(RiskManagementCommand::GetStatus);
        }
        
        if let Some(tx) = &self.execution_engine_tx {
            let _ = tx.send(ExecutionEngineCommand::GetStatus);
        }
        
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
        if let Some(tx) = &self.risk_manager_tx {
            let _ = tx.send(RiskManagementCommand::HaltSystem);
            info!("Halt command sent to risk manager");
        }
        
        Ok(())
    }
    
    /// Process resume command
    async fn process_resume_command(&self, user_id: i64) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing resume command for user {}", user_id);
        
        // Send resume command to risk manager
        if let Some(tx) = &self.risk_manager_tx {
            let _ = tx.send(RiskManagementCommand::ResumeSystem);
            info!("Resume command sent to risk manager");
        }
        
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
        if let Some(tx) = &self.risk_manager_tx {
            let _ = tx.send(RiskManagementCommand::SetRiskPercentage(percentage));
            info!("Set risk command sent to risk manager: {}%", percentage);
        }
        
        Ok(())
    }
    
    /// Process set mode command
    async fn process_set_mode_command(&mut self, user_id: i64, mode: crate::components::shadow_trading::ExecutionEngineType) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing set mode command for user {}: {:?}", user_id, mode);
        
        // For live mode, require additional confirmation
        if matches!(mode, crate::components::shadow_trading::ExecutionEngineType::Live) {
            // In a real implementation, we would ask for confirmation code
            // For now, we'll proceed directly
        }
        
        // Send set mode command to execution engine
        if let Some(tx) = &self.execution_engine_tx {
            let _ = tx.send(ExecutionEngineCommand::SetMode(mode));
            info!("Set mode command sent to execution engine: {:?}", mode);
        }
        
        Ok(())
    }
    
    /// Generate daily confirmation code
    pub fn generate_daily_confirmation_code(&mut self) -> String {
        self.security_validator.generate_daily_confirmation_code()
    }
    
    /// Check if processor is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}