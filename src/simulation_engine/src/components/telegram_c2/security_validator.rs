//! Security validator for Telegram C2 commands

use tracing::{info, debug, warn, error};
use std::collections::HashMap;

/// Security validator for C2 commands
pub struct SecurityValidator {
    authorized_users: Vec<i64>,
    daily_confirmation_codes: HashMap<String, i64>, // code -> expiration timestamp
    is_initialized: bool,
}

impl SecurityValidator {
    /// Create a new security validator
    pub fn new(authorized_users: Vec<i64>) -> Self {
        Self {
            authorized_users,
            daily_confirmation_codes: HashMap::new(),
            is_initialized: false,
        }
    }
    
    /// Initialize the security validator
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.is_initialized = true;
        info!("Security Validator initialized with {} authorized users", self.authorized_users.len());
        Ok(())
    }
    
    /// Check if user is authorized
    pub fn is_user_authorized(&self, user_id: i64) -> bool {
        if !self.is_initialized {
            return false;
        }
        
        self.authorized_users.contains(&user_id)
    }
    
    /// Generate daily confirmation code
    pub fn generate_daily_confirmation_code(&mut self) -> String {
        use chrono::Utc;
        
        // Generate a simple code based on date and a secret
        let now = Utc::now();
        let date_str = now.format("%Y%m%d").to_string();
        let code = format!("AEGIS{}", date_str);
        
        // Store with 24-hour expiration
        let expiration = now.timestamp() + 24 * 60 * 60;
        self.daily_confirmation_codes.insert(code.clone(), expiration);
        
        info!("Generated daily confirmation code: {}", code);
        code
    }
    
    /// Validate confirmation code
    pub fn validate_confirmation_code(&mut self, code: &str) -> bool {
        use chrono::Utc;
        
        if !self.is_initialized {
            return false;
        }
        
        let now = Utc::now().timestamp();
        
        // Clean expired codes
        self.daily_confirmation_codes.retain(|_, &mut expiration| expiration > now);
        
        // Check if code exists and is not expired
        if let Some(&expiration) = self.daily_confirmation_codes.get(code) {
            if now <= expiration {
                // Remove used code to prevent reuse
                self.daily_confirmation_codes.remove(code);
                info!("Valid confirmation code used: {}", code);
                true
            } else {
                warn!("Expired confirmation code attempted: {}", code);
                false
            }
        } else {
            warn!("Invalid confirmation code attempted: {}", code);
            false
        }
    }
    
    /// Validate sensitive command
    pub fn validate_sensitive_command(&mut self, user_id: i64, command: &str, confirmation_code: Option<String>) -> bool {
        if !self.is_user_authorized(user_id) {
            warn!("Unauthorized user {} attempted sensitive command: {}", user_id, command);
            return false;
        }
        
        // Check if confirmation code is required for this command
        match command {
            "halt" | "mode_live" => {
                if let Some(code) = confirmation_code {
                    if self.validate_confirmation_code(&code) {
                        info!("Sensitive command '{}' approved for user {}", command, user_id);
                        true
                    } else {
                        warn!("Invalid confirmation code for sensitive command '{}' by user {}", command, user_id);
                        false
                    }
                } else {
                    warn!("Confirmation code required for sensitive command '{}' by user {}", command, user_id);
                    false
                }
            }
            _ => {
                // Non-sensitive commands don't require confirmation
                true
            }
        }
    }
    
    /// Add authorized user
    pub fn add_authorized_user(&mut self, user_id: i64) {
        if !self.authorized_users.contains(&user_id) {
            self.authorized_users.push(user_id);
            info!("Added authorized user: {}", user_id);
        }
    }
    
    /// Remove authorized user
    pub fn remove_authorized_user(&mut self, user_id: i64) {
        if let Some(pos) = self.authorized_users.iter().position(|&x| x == user_id) {
            self.authorized_users.remove(pos);
            info!("Removed authorized user: {}", user_id);
        }
    }
    
    /// Get authorized users
    pub fn get_authorized_users(&self) -> &[i64] {
        &self.authorized_users
    }
    
    /// Check if validator is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}