//! Security validator for Telegram C2 commands
//!
//! Two things gate a hand to live trading: the sender's id, and a single-use
//! confirmation code. Both have to fail closed, and both have to say so in the log.

use std::collections::HashMap;
use tracing::{error, info, warn};

/// How long an issued confirmation code stays usable. Short on purpose: the code is
/// read out of a log file, and a code that lives for a day is a code that can be found
/// later.
const CONFIRMATION_CODE_TTL_SECONDS: i64 = 15 * 60;

/// Alphabet without the glyphs that get misread on a phone screen (0/O, 1/I/l), since
/// the code is copied from a log into Telegram.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 10;

/// Security validator for C2 commands
pub struct SecurityValidator {
    authorized_users: Vec<i64>,
    /// code -> expiration timestamp (seconds). Single use: a code is removed the moment
    /// it is presented, valid or not.
    confirmation_codes: HashMap<String, i64>,
    is_initialized: bool,
}

impl SecurityValidator {
    /// Create a new Security Validator
    pub fn new(authorized_users: Vec<i64>) -> Self {
        Self {
            authorized_users,
            confirmation_codes: HashMap::new(),
            is_initialized: false,
        }
    }

    /// Initialize the security validator
    ///
    /// An empty allowlist is refused rather than accepted: every command - including
    /// `/halt` - would be rejected afterwards, and a component that is "running" while
    /// deaf is the failure mode an operator does not want to discover under load.
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.authorized_users.is_empty() {
            error!(
                "Security Validator cannot start with no authorized users: every command, \
                 including /halt, would be refused"
            );
            return Err("Telegram C2 needs at least one authorized user id".into());
        }

        self.is_initialized = true;
        info!(
            "Security Validator initialized with {} authorized user(s)",
            self.authorized_users.len()
        );
        Ok(())
    }

    /// Check if user is authorized
    pub fn is_user_authorized(&self, user_id: i64) -> bool {
        if !self.is_initialized {
            return false;
        }

        self.authorized_users.contains(&user_id)
    }

    /// Issue a single-use confirmation code.
    ///
    /// It used to be `AEGIS{YYYYMMDD}` - computable by anyone who knew the format, for
    /// any future date, which is exactly the credential that authorizes `/mode live`
    /// and `/halt`. The log line is the delivery channel for this standalone build, so
    /// it prints the code once, at issue time; it is never printed again, and a used
    /// code cannot be replayed.
    pub fn issue_confirmation_code(&mut self) -> String {
        use chrono::Utc;
        use rand::Rng;

        let code: String = {
            let mut rng = rand::thread_rng();
            (0..CODE_LENGTH)
                .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
                .collect()
        };

        let expiration = Utc::now().timestamp() + CONFIRMATION_CODE_TTL_SECONDS;
        self.confirmation_codes.insert(code.clone(), expiration);

        info!(
            "Issued confirmation code {} (single use, expires in {} minutes)",
            code,
            CONFIRMATION_CODE_TTL_SECONDS / 60
        );
        code
    }

    /// Validate a confirmation code, consuming it
    pub fn validate_confirmation_code(&mut self, code: &str) -> bool {
        use chrono::Utc;

        if !self.is_initialized {
            warn!("Confirmation code presented before the validator was initialized; refused");
            return false;
        }

        let now = Utc::now().timestamp();
        let accepted = match self.confirmation_codes.remove(code) {
            Some(expiration) if now <= expiration => {
                info!("Confirmation code accepted and consumed");
                true
            }
            Some(_) => {
                warn!("Expired confirmation code attempted");
                false
            }
            None => {
                // Echoing the attempt is useful: repeated failures are the signal that
                // someone is guessing, and an unknown code is not a secret.
                warn!("Invalid confirmation code attempted: {}", code);
                false
            }
        };

        // Codes nobody presented expire in the background, so the map cannot grow one
        // entry per issued code over the life of the process.
        self.confirmation_codes.retain(|_, expiration| *expiration > now);
        accepted
    }

    /// Validate sensitive command
    pub fn validate_sensitive_command(
        &mut self,
        user_id: i64,
        command: &str,
        confirmation_code: Option<String>,
    ) -> bool {
        if !self.is_user_authorized(user_id) {
            warn!(
                "Unauthorized user {} attempted sensitive command: {}",
                user_id, command
            );
            return false;
        }

        match command {
            "halt" | "mode_live" => match confirmation_code {
                Some(code) if self.validate_confirmation_code(&code) => {
                    info!("Sensitive command '{}' approved for user {}", command, user_id);
                    true
                }
                Some(_) => {
                    warn!(
                        "Invalid confirmation code for sensitive command '{}' by user {}",
                        command, user_id
                    );
                    false
                }
                None => {
                    warn!(
                        "Confirmation code required for sensitive command '{}' by user {}",
                        command, user_id
                    );
                    false
                }
            },
            // Non-sensitive commands don't require confirmation.
            _ => true,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn validator() -> SecurityValidator {
        let mut validator = SecurityValidator::new(vec![42]);
        validator
            .initialize()
            .expect("an authorized user is configured");
        validator
    }

    #[test]
    fn an_empty_allowlist_refuses_to_start() {
        let mut validator = SecurityValidator::new(vec![]);
        let error = validator
            .initialize()
            .expect_err("a C2 with no authorized user is deaf");
        assert!(error.to_string().contains("authorized user"), "{}", error);
        assert!(!validator.is_initialized());
        assert!(!validator.is_user_authorized(42));
    }

    #[test]
    fn codes_are_not_derivable_from_the_date() {
        let mut validator = validator();
        let code = validator.issue_confirmation_code();

        assert_eq!(code.len(), CODE_LENGTH);
        assert!(
            code.chars().all(|c| CODE_ALPHABET.contains(&(c as u8))),
            "the alphabet excludes look-alike glyphs"
        );

        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        assert!(!code.contains("AEGIS"), "the old scheme must not reappear");
        assert!(!code.contains(&today));

        assert_ne!(code, validator.issue_confirmation_code(), "codes must not repeat");
    }

    #[test]
    fn a_code_is_single_use_and_expires() {
        let mut validator = validator();

        let code = validator.issue_confirmation_code();
        assert!(validator.validate_confirmation_code(&code));
        assert!(
            !validator.validate_confirmation_code(&code),
            "a consumed code cannot be replayed"
        );
        assert!(!validator.validate_confirmation_code("NOPE2345"));

        let stale = validator.issue_confirmation_code();
        validator
            .confirmation_codes
            .insert(stale.clone(), chrono::Utc::now().timestamp() - 1);
        assert!(
            !validator.validate_confirmation_code(&stale),
            "an expired code is refused"
        );
    }

    #[test]
    fn commands_before_initialization_are_refused() {
        let mut validator = SecurityValidator::new(vec![42]);
        assert!(
            !validator.validate_confirmation_code("WHATEVER12"),
            "an uninitialized validator fails closed"
        );
        assert!(!validator.is_user_authorized(42));
    }

    #[test]
    fn sensitive_commands_need_a_fresh_code_from_the_authorized_user() {
        let mut validator = validator();

        let code = validator.issue_confirmation_code();
        assert!(
            !validator.validate_sensitive_command(7, "halt", Some(code.clone())),
            "another user is refused, and their attempt must not burn the code"
        );
        assert!(
            !validator.validate_sensitive_command(42, "mode_live", None),
            "a missing code is refused"
        );
        assert!(validator.validate_sensitive_command(42, "halt", Some(code.clone())));
        assert!(
            !validator.validate_sensitive_command(42, "halt", Some(code)),
            "the same code does not work twice"
        );
        assert!(
            validator.validate_sensitive_command(42, "status", None),
            "read-only commands need no code"
        );
    }

    #[test]
    fn allowlist_changes_take_effect_once_applied() {
        let mut validator = validator();
        validator.remove_authorized_user(42);
        assert!(!validator.is_user_authorized(42));
        validator.add_authorized_user(99);
        validator.add_authorized_user(99);
        assert_eq!(validator.get_authorized_users().to_vec(), vec![99]);
    }
}
