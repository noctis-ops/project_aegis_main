//! Smart Order Router implementation

use crate::core::{Order, OrderType, TimeInForce, ExecutionError, RateLimitInfo};
use crate::security::BinanceSignature;
use crate::integration::ErrorHandler;
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
            
            // A GTX entry that would cross the book is answered with HTTP 200 and an
            // EXPIRED/REJECTED status, so the status line alone never proves the order
            // reached the exchange — without this check the lifecycle manager would
            // track an order that does not exist on the book.
            if let Some(err) = Self::engine_rejection(response_text.as_str(), true) {
                return Err(err);
            }
            
            Ok(())
        } else {
            Err(Self::classify_rejection(response, "Limit order").await)
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
            
            // Exits are never routine: a rejected market order means the position is
            // still open and unprotected, so it must not be swallowed.
            if let Some(err) = Self::engine_rejection(response_text.as_str(), false) {
                return Err(err);
            }
            
            Ok(())
        } else {
            Err(Self::classify_rejection(response, "Market order").await)
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
            
            if let Some(err) = Self::engine_rejection(response_text.as_str(), false) {
                return Err(err);
            }
            
            Ok(())
        } else {
            Err(Self::classify_rejection(response, "Stop market order").await)
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
            
            if let Some(err) = Self::engine_rejection(response_text.as_str(), true) {
                return Err(err);
            }
            
            Ok(())
        } else {
            Err(Self::classify_rejection(response, "Take profit order").await)
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
            // No matching-engine check here: on a cancel the `status` field carries
            // the order's terminal state (CANCELED), not an acceptance verdict.
            debug!("Cancel order response: {}", response_text);
            Ok(())
        } else {
            Err(Self::classify_rejection(response, "Cancel order").await)
        }
    }
    
    /// Classify a non-2xx order response into a typed `ExecutionError`.
    ///
    /// `reqwest::Response::text()` takes `self` by value, so the status line and the
    /// `Retry-After` header must be read *before* the body is drained — that ordering
    /// is the actual defect behind the moved-value errors, not something to paper
    /// over at each call site. Keeping it in one routine is also what makes every
    /// routed order type (limit, market, stop, take profit, cancel) report failures
    /// through one taxonomy, which is what Layer 4's circuit breakers and the
    /// reconciliation engine key off.
    async fn classify_rejection(
        response: reqwest::Response,
        order_kind: &'static str,
    ) -> ExecutionError {
        let status_code = response.status().as_u16();
        let retry_after_secs = response
            .headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(RATE_LIMIT_DEFAULT_BACKOFF_SECS);

        let body = match response.text().await {
            Ok(body) => body,
            // The body never arrived: a transport failure, not an exchange verdict.
            // The order may still be live, so it must be reconciled with the
            // exchange rather than assumed rejected.
            Err(err) => {
                error!(
                    "{}: unreadable error body (HTTP {}): {}",
                    order_kind, status_code, err
                );
                return ExecutionError::NetworkError(err);
            }
        };

        // 429 = weight/IP rate limit, 418 = temporary ban. Echo the ban window so
        // the local weight accounting and the caller's back-off decision agree.
        if status_code == 429 || status_code == 418 {
            warn!(
                "{} is rate limited (HTTP {}), backing off for {}s",
                order_kind, status_code, retry_after_secs
            );
            return ExecutionError::RateLimitExceeded(format!(
                "HTTP {} (retry after {}s): {}",
                status_code, retry_after_secs, body
            ));
        }

        // Preferred path: Binance's structured {"code": -2019, "msg": "..."} payload,
        // mapped by the crate's canonical classifier (integration::ErrorHandler) so
        // the exchange error table lives in a single component for every caller.
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(body.as_str()) {
            if let Some(code) = payload.get("code").and_then(serde_json::Value::as_i64) {
                let msg = payload
                    .get("msg")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(body.as_str());
                return ErrorHandler::handle_binance_error(code as i32, msg);
            }
        }

        // Fallback for answers without a numeric code (proxies, HTML error pages):
        // match the message text, then degrade to the status code.
        error!("{} failed (HTTP {}): {}", order_kind, status_code, body);

        if body.contains("Margin is insufficient") {
            ExecutionError::InsufficientMargin(body)
        } else if body.contains("Post-Only") || body.contains("Post Only") {
            ExecutionError::OrderRejected(body)
        } else {
            ExecutionError::Other(format!("HTTP {}: {}", status_code, body))
        }
    }
    
    /// Detect a matching-engine rejection carried inside a 2xx order reply.
    ///
    /// Binance answers a rejected GTX entry with HTTP 200 and a `status` of
    /// `EXPIRED`/`REJECTED`, so a 2xx alone never proves the order reached the book.
    ///
    /// `routine` marks the paths where a rejection is normal market behaviour
    /// (post-only entries that would cross, take profits already beyond the market):
    /// those map to `OrderRejected`, which the engine absorbs and keeps trading on.
    /// Protective paths (market exits, stop losses) pass `false` so the failure
    /// surfaces — a position left unprotected must never look like a routine miss.
    fn engine_rejection(body: &str, routine: bool) -> Option<ExecutionError> {
        let payload = match serde_json::from_str::<serde_json::Value>(body) {
            Ok(payload) => payload,
            // An unparseable answer tells us nothing; the user data stream stays the
            // authority on real order state, so trust the 2xx here.
            Err(_) => return None,
        };
        
        let status = match payload.get("status").and_then(serde_json::Value::as_str) {
            Some(status) => status,
            None => return None,
        };
        
        let rejected = status == "REJECTED" || (routine && status == "EXPIRED");
        if !rejected {
            return None;
        }
        
        if routine {
            warn!("Order rejected by matching engine ({}): {}", status, body);
            Some(ExecutionError::OrderRejected(body.to_string()))
        } else {
            error!(
                "Protective order rejected by matching engine ({}): {}",
                status, body
            );
            Some(ExecutionError::Other(format!(
                "order rejected by matching engine ({}): {}",
                status, body
            )))
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
