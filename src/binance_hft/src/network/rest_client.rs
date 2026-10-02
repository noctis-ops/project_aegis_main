//! REST API client for Binance Futures

use crate::core::{HftError, OrderBookSnapshot};
use reqwest;
use tracing::info;
use crate::core::constants::*;

/// REST client for Binance Futures API.
///
/// Cheap to clone: wraps `reqwest::Client`, which is reference-counted
/// internally and shares one connection pool across all clones.
#[derive(Clone)]
pub struct RestClient {
    client: reqwest::Client,
    base_url: String,
}

impl RestClient {
    /// Create a new REST client
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: BINANCE_FUTURES_REST_URL.to_string(),
        }
    }
    
    /// Fetch order book snapshot for a symbol
    pub async fn get_orderbook_snapshot(&self, symbol: &str, limit: usize) -> Result<OrderBookSnapshot, HftError> {
        let url = format!("{}/fapi/v1/depth", self.base_url);
        
        info!("Fetching order book snapshot for {} with limit {}", symbol, limit);
        
        let response = self.client
            .get(&url)
            .query(&[("symbol", symbol), ("limit", &limit.to_string())])
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(HftError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        let snapshot: OrderBookSnapshot = response.json().await?;
        Ok(snapshot)
    }
    
    /// Cancel all open orders for a symbol (emergency function)
    pub async fn cancel_all_orders(&self, symbol: &str, api_key: &str) -> Result<(), HftError> {
        let url = format!("{}/fapi/v1/allOpenOrders", self.base_url);
        
        let timestamp = chrono::Utc::now().timestamp_millis();
        
        let response = self.client
            .delete(&url)
            .header("X-MBX-APIKEY", api_key)
            .query(&[("symbol", symbol), ("timestamp", &timestamp.to_string())])
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(HftError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())));
        }
        
        Ok(())
    }
}