//! Core data types for the Execution Engine

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trade intent received from Layer 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeIntent {
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub size: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub time_to_live: u64, // milliseconds
    pub timestamp: u64,
}

/// Order side enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OrderType {
    Limit,
    Market,
    StopMarket,
    TakeProfitMarket,
}

/// Time in force enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TimeInForce {
    GTC, // Good Til Canceled
    IOC, // Immediate or Cancel
    FOK, // Fill or Kill
    GTX, // Good Til Crossing (Post Only)
}

/// Order status enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Expired,
    Rejected,
}

/// Order structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Option<String>, // Binance order ID
    pub client_order_id: String,   // Our unique order ID (UUID)
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: f64,
    pub quantity: f64,
    pub time_in_force: TimeInForce,
    /// Time-to-live in milliseconds; 0 = no TTL (good-til-canceled style,
    /// used for protective stop-loss / take-profit orders).
    pub time_to_live: u64,
    pub status: OrderStatus,
    pub filled_quantity: f64,
    pub avg_price: f64,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Order {
    /// Create a new order from trade intent
    pub fn from_trade_intent(intent: &TradeIntent) -> Self {
        Self {
            order_id: None,
            client_order_id: Uuid::new_v4().to_string(),
            symbol: intent.symbol.clone(),
            side: intent.side,
            order_type: OrderType::Limit,
            price: intent.price,
            quantity: intent.size,
            time_in_force: TimeInForce::GTX, // Post Only
            time_to_live: intent.time_to_live,
            status: OrderStatus::New,
            filled_quantity: 0.0,
            avg_price: 0.0,
            created_at: intent.timestamp,
            updated_at: intent.timestamp,
        }
    }
    
    /// Create a stop loss order
    pub fn stop_loss_order(symbol: String, side: OrderSide, stop_price: f64, quantity: f64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            order_id: None,
            client_order_id: format!("sl_{}", Uuid::new_v4()),
            symbol,
            side,
            order_type: OrderType::StopMarket,
            price: stop_price,
            quantity,
            time_in_force: TimeInForce::GTC,
            time_to_live: 0, // Protective orders do not expire
            status: OrderStatus::New,
            filled_quantity: 0.0,
            avg_price: 0.0,
            created_at: now,
            updated_at: now,
        }
    }
    
    /// Create a take profit order
    pub fn take_profit_order(symbol: String, side: OrderSide, take_profit_price: f64, quantity: f64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            order_id: None,
            client_order_id: format!("tp_{}", Uuid::new_v4()),
            symbol,
            side,
            order_type: OrderType::TakeProfitMarket,
            price: take_profit_price,
            quantity,
            time_in_force: TimeInForce::GTC,
            time_to_live: 0, // Protective orders do not expire
            status: OrderStatus::New,
            filled_quantity: 0.0,
            avg_price: 0.0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Execution report from Binance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "c")]
    pub client_order_id: String,
    #[serde(rename = "S")]
    pub side: String,
    #[serde(rename = "o")]
    pub order_type: String,
    #[serde(rename = "f")]
    pub time_in_force: String,
    #[serde(rename = "q")]
    pub original_quantity: String,
    #[serde(rename = "p")]
    pub original_price: String,
    #[serde(rename = "ap")]
    pub average_price: String,
    #[serde(rename = "sp")]
    pub stop_price: String,
    #[serde(rename = "x")]
    pub execution_type: String,
    #[serde(rename = "X")]
    pub order_status: String,
    #[serde(rename = "i")]
    pub order_id: u64,
    #[serde(rename = "l")]
    pub last_executed_quantity: String,
    #[serde(rename = "z")]
    pub cumulative_filled_quantity: String,
    #[serde(rename = "L")]
    pub last_executed_price: String,
    #[serde(rename = "n")]
    pub commission: String,
    #[serde(rename = "N")]
    pub commission_asset: String,
    #[serde(rename = "T")]
    pub transaction_time: u64,
    #[serde(rename = "t")]
    pub trade_id: u64,
}

/// Account update from Binance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdate {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "a")]
    pub account_info: AccountInfo,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(rename = "m")]
    pub margin_info: Vec<MarginInfo>,
    #[serde(rename = "B")]
    pub balances: Vec<Balance>,
}

/// Margin information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginInfo {
    #[serde(rename = "a")]
    pub asset: String,
    #[serde(rename = "wb")]
    pub wallet_balance: String,
    #[serde(rename = "cw")]
    pub cross_wallet_balance: String,
}

/// Balance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    #[serde(rename = "a")]
    pub asset: String,
    #[serde(rename = "wb")]
    pub wallet_balance: String,
    #[serde(rename = "cw")]
    pub cross_wallet_balance: String,
}

/// Position risk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionRisk {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "positionAmt")]
    pub position_amount: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "unRealizedProfit")]
    pub unrealized_profit: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "maxNotionalValue")]
    pub max_notional_value: String,
    #[serde(rename = "marginType")]
    pub margin_type: String,
    #[serde(rename = "isolatedMargin")]
    pub isolated_margin: String,
    #[serde(rename = "isAutoAddMargin")]
    pub is_auto_add_margin: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
}

/// Rate limit information
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub used_weight: u64,
    pub max_weight: u64,
    pub last_updated: u64,
}
