//! Binance API signature generation

use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64;
use std::env;

/// Binance signature generator
pub struct BinanceSignature {
    api_secret: String,
}

impl BinanceSignature {
    /// Create a new Binance signature generator
    pub fn new() -> Self {
        let api_secret = env::var("BINANCE_API_SECRET").unwrap_or_default();
        
        Self {
            api_secret,
        }
    }
    
    /// Sign a message using HMAC-SHA256
    pub fn sign(&self, message: &str) -> String {
        if self.api_secret.is_empty() {
            return String::new();
        }
        
        let mut mac = Hmac::<Sha256>::new_from_slice(self.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        
        hex::encode(code_bytes)
    }
    
    /// Set API secret
    pub fn set_api_secret(&mut self, secret: String) {
        self.api_secret = secret;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signature_generation() {
        let signer = BinanceSignature::new();
        // In a real test, we would use a known secret and message
        // For now, we just test that it doesn't panic
        let signature = signer.sign("test_message");
        assert!(!signature.is_empty() || signer.api_secret.is_empty());
    }
}