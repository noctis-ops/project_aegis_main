//! Smart Order Router implementation

use crate::core::{Order, OrderType, TimeInForce, ExecutionError, RateLimitInfo};
use crate::security::BinanceSignature;
use reqwest;
use tracing::{info, warn, error, debug};
use crate::core::constants::*;

/// Smart Order Router
pub struct SmartOrderRouter {
    http_client: reqwest::Client,
    binance_signature: BinanceSignature,
    rate_limit_info: RateLimitInfo,
    api_key: String,
}

impl SmartOrderRouter {
    /// Create a new Smart Order Router
    pub fn new() -> Self {
        let api_key = std::env::var("BINANCE_API_KEY").unwrap_or_default();
        
        Self {
            http_client: reqwest::Client::new(),
            binance_signature: BinanceSignature::new(),
            rate_limit_info: RateLimitInfo {
                used_weight: 0,
                max_weight: MAX_WEIGHT_PER_MINUTE,
                last_updated: 0,
            },
            api_key,
        }
    }
    
    /// Initialize order API connection
    pub async fn initialize_order_api(&mut self) -> Result<(), ExecutionError> {
        info!("Initializing Binance Order API connection");
        // In a real implementation, this would establish WebSocket connection
        // to the order API endpoint
        Ok(())
    }
    
    /// Route an order based on its type and market conditions.
    ///
    /// Borrows the order: routing only reads its fields, and the caller
    /// keeps ownership for lifecycle tracking — no clone on the hot path.
    pub async fn route_order(&mut self, order: &Order) -> Result<(), ExecutionError> {
        debug!("Routing order: {:?}", order);
        
        match order.order_type {
            OrderType::Limit => {
                // Entry orders are sent as limit orders with GTX (Post Only)
                self.send_limit_order(order).await
            }
            OrderType::Market => {
                // Emergency exits are sent as market orders
                self.send_market_order(order).await
            }
            OrderType::StopMarket => {
                // Stop loss orders
                self.send_stop_market_order(order).await
            }
            OrderType::TakeProfitMarket => {
                // Take profit orders
                self.send_take_profit_order(order).await
            }
        }
    }
    
    /// Send a limit order
    async fn send_limit_order(&mut self, order: &Order) -> Result<(), ExecutionError> {
        info!("Sending limit order for symbol: {}", order.symbol);
        
        // Build query parameters
        let mut params = vec![
            ("symbol", order.symbol.clone()),
            ("side", match order.side {
                crate::core::OrderSide::Buy => "BUY".to_string(),
                crate::core::OrderSide::Sell => "SELL".to_string(),
            }),
            ("type", "LIMIT".to_string()),
            ("quantity", order.quantity.to_string()),
            ("price", order.price.to_string()),
            ("timeInForce", match order.time_in_force {
                TimeInForce::GTX => "GTX".to_string(),
                _ => "GTX".to_string(), // Force GTX for entries
            }),
            ("newClientOrderId", order.client_order_id.clone()),
        ];
        
        // Add timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        params.push(("timestamp", timestamp.to_string()));
        
        // Sign the request
        let query_string = self.build_query_string(&params);
        let signature = self.binance_signature.sign(&query_string);
        params.push(("signature", signature));
        
        // Send the request
        let url = format!("{}/fapi/v1/order", BINANCE_FUTURES_REST_URL);
        
        let response = self.http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .form(&params)
            .send()
            .await?;
            
        // Update rate limit info
        self.update_rate_limit_info(&response);
        
        if response.status().is_success() {
            let response_text = response.text().await?;
            debug!("Order response: {}", response_text);
            Ok(())
        } else {
            let error_text = response.text().await?;
            error!("Order failed: {}", error_text);
            
            // Parse error and handle specific cases
            if error_text.contains("Margin is insufficient") {
                return Err(ExecutionError::InsufficientMargin(error_text));
            } else if error_text.contains("Post-Only orders will be rejected") {
                return Err(ExecutionError::OrderRejected(error_text));
            }
            
            Err(ExecutionError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())))
        }
    }
    
    /// Send a market order (emergency exit)
    async fn send_market_order(&mut self, order: &Order) -> Result<(), ExecutionError> {
        info!("Sending market order for symbol: {}", order.symbol);
        
        // For emergency exits, we might want to use IOC with slightly worse price
        // to ensure execution while protecting against extreme slippage
        
        let mut params = vec![
            ("symbol", order.symbol.clone()),
            ("side", match order.side {
                crate::core::OrderSide::Buy => "BUY".to_string(),
                crate::core::OrderSide::Sell => "SELL".to_string(),
            }),
            ("type", "MARKET".to_string()),
            ("quantity", order.quantity.to_string()),
            ("newClientOrderId", order.client_order_id.clone()),
        ];
        
        // Add timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        params.push(("timestamp", timestamp.to_string()));
        
        // Sign the request
        let query_string = self.build_query_string(&params);
        let signature = self.binance_signature.sign(&query_string);
        params.push(("signature", signature));
        
        // Send the request
        let url = format!("{}/fapi/v1/order", BINANCE_FUTURES_REST_URL);
        
        let response = self.http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .form(&params)
            .send()
            .await?;
            
        // Update rate limit info
        self.update_rate_limit_info(&response);
        
        if response.status().is_success() {
            let response_text = response.text().await?;
            debug!("Market order response: {}", response_text);
            Ok(())
        } else {
            let error_text = response.text().await?;
            error!("Market order failed: {}", error_text);
            Err(ExecutionError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())))
        }
    }
    
    /// Send a stop market order
    async fn send_stop_market_order(&mut self, order: &Order) -> Result<(), ExecutionError> {
        info!("Sending stop market order for symbol: {}", order.symbol);
        
        let mut params = vec![
            ("symbol", order.symbol.clone()),
            ("side", match order.side {
                crate::core::OrderSide::Buy => "BUY".to_string(),
                crate::core::OrderSide::Sell => "SELL".to_string(),
            }),
            ("type", "STOP_MARKET".to_string()),
            ("quantity", order.quantity.to_string()),
            ("stopPrice", order.price.to_string()),
            ("newClientOrderId", order.client_order_id.clone()),
        ];
        
        // Add timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        params.push(("timestamp", timestamp.to_string()));
        
        // Sign the request
        let query_string = self.build_query_string(&params);
        let signature = self.binance_signature.sign(&query_string);
        params.push(("signature", signature));
        
        // Send the request
        let url = format!("{}/fapi/v1/order", BINANCE_FUTURES_REST_URL);
        
        let response = self.http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .form(&params)
            .send()
            .await?;
            
        // Update rate limit info
        self.update_rate_limit_info(&response);
        
        if response.status().is_success() {
            let response_text = response.text().await?;
            debug!("Stop market order response: {}", response_text);
            Ok(())
        } else {
            let error_text = response.text().await?;
            error!("Stop market order failed: {}", error_text);
            Err(ExecutionError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())))
        }
    }
    
    /// Send a take profit order
    async fn send_take_profit_order(&mut self, order: &Order) -> Result<(), ExecutionError> {
        info!("Sending take profit order for symbol: {}", order.symbol);
        
        let mut params = vec![
            ("symbol", order.symbol.clone()),
            ("side", match order.side {
                crate::core::OrderSide::Buy => "BUY".to_string(),
                crate::core::OrderSide::Sell => "SELL".to_string(),
            }),
            ("type", "TAKE_PROFIT_MARKET".to_string()),
            ("quantity", order.quantity.to_string()),
            ("stopPrice", order.price.to_string()),
            ("newClientOrderId", order.client_order_id.clone()),
        ];
        
        // Add timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        params.push(("timestamp", timestamp.to_string()));
        
        // Sign the request
        let query_string = self.build_query_string(&params);
        let signature = self.binance_signature.sign(&query_string);
        params.push(("signature", signature));
        
        // Send the request
        let url = format!("{}/fapi/v1/order", BINANCE_FUTURES_REST_URL);
        
        let response = self.http_client
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .form(&params)
            .send()
            .await?;
            
        // Update rate limit info
        self.update_rate_limit_info(&response);
        
        if response.status().is_success() {
            let response_text = response.text().await?;
            debug!("Take profit order response: {}", response_text);
            Ok(())
        } else {
            let error_text = response.text().await?;
            error!("Take profit order failed: {}", error_text);
            Err(ExecutionError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())))
        }
    }
    
    /// Cancel an order
    pub async fn cancel_order(&mut self, symbol: String, client_order_id: String) -> Result<(), ExecutionError> {
        info!("Canceling order {} for symbol: {}", client_order_id, symbol);
        
        let mut params = vec![
            ("symbol", symbol),
            ("origClientOrderId", client_order_id),
        ];
        
        // Add timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        params.push(("timestamp", timestamp.to_string()));
        
        // Sign the request
        let query_string = self.build_query_string(&params);
        let signature = self.binance_signature.sign(&query_string);
        params.push(("signature", signature));
        
        // Send the request
        let url = format!("{}/fapi/v1/order", BINANCE_FUTURES_REST_URL);
        
        let response = self.http_client
            .delete(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .form(&params)
            .send()
            .await?;
            
        // Update rate limit info
        self.update_rate_limit_info(&response);
        
        if response.status().is_success() {
            let response_text = response.text().await?;
            debug!("Cancel order response: {}", response_text);
            Ok(())
        } else {
            let error_text = response.text().await?;
            error!("Cancel order failed: {}", error_text);
            Err(ExecutionError::NetworkError(reqwest::Error::from(response.error_for_status().unwrap_err())))
        }
    }
    
    /// Check rate limits
    pub fn check_rate_limits(&self) -> bool {
        // Check if we're approaching rate limits
        let remaining_weight = self.rate_limit_info.max_weight.saturating_sub(self.rate_limit_info.used_weight);
        
        // If remaining weight is less than 10%, don't send non-critical orders
        if remaining_weight < (self.rate_limit_info.max_weight / 10) {
            warn!("Approaching rate limit: {} weight remaining", remaining_weight);
            false
        } else {
            true
        }
    }
    
    /// Build query string from parameters.
    ///
    /// Parameter keys are `&'static str` literals at every call site, so the
    /// signature takes `(&str, String)` pairs — one canonical form instead
    /// of five mismatched conversions.
    fn build_query_string(&self, params: &[(&str, String)]) -> String {
        params.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    }
    
    /// Update rate limit information from response headers
    fn update_rate_limit_info(&mut self, response: &reqwest::Response) {
        if let Some(weight_header) = response.headers().get("x-mbx-used-weight-1m") {
            if let Ok(weight_str) = weight_header.to_str() {
                if let Ok(weight) = weight_str.parse::<u64>() {
                    self.rate_limit_info.used_weight = weight;
                    self.rate_limit_info.last_updated = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                }
            }
        }
    }
}
