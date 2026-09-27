//! Core data types for the HFT system

use serde::{Deserialize, Serialize};

/// Price level in the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: f64,
}

/// Order book snapshot from Binance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub last_update_id: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

/// Order book update message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookUpdate {
    pub event_type: String,
    pub event_time: u64,
    pub symbol: String,
    pub first_update_id: u64,
    pub final_update_id: u64,
    pub previous_final_update_id: Option<u64>,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

/// Aggregated trade message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggTrade {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "a")]
    pub aggregated_trade_id: u64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "f")]
    pub first_trade_id: u64,
    #[serde(rename = "l")]
    pub last_trade_id: u64,
    #[serde(rename = "T")]
    pub trade_time: u64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Mark price update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkPriceUpdate {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "p")]
    pub mark_price: String,
    #[serde(rename = "i")]
    pub index_price: String,
    #[serde(rename = "P")]
    pub estimated_settle_price: String,
    #[serde(rename = "r")]
    pub funding_rate: String,
    #[serde(rename = "T")]
    pub next_funding_time: u64,
}

/// Force order (liquidation) message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceOrder {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "o")]
    pub order: LiquidationOrder,
}

/// Liquidation order details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidationOrder {
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "S")]
    pub side: String,
    #[serde(rename = "o")]
    pub order_type: String,
    #[serde(rename = "f")]
    pub time_in_force: String,
    #[serde(rename = "q")]
    pub original_quantity: String,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "ap")]
    pub average_price: String,
    #[serde(rename = "X")]
    pub order_status: String,
    #[serde(rename = "l")]
    pub order_last_filled_quantity: String,
    #[serde(rename = "z")]
    pub order_filled_accumulated_quantity: String,
    #[serde(rename = "T")]
    pub order_trade_time: u64,
}

/// Internal representation of order book levels
#[derive(Debug, Clone, Copy)]
pub struct OrderBookLevel {
    pub price: f64,
    pub quantity: f64,
}

impl OrderBookLevel {
    pub fn new(price: f64, quantity: f64) -> Self {
        Self { price, quantity }
    }
    
    pub fn is_empty(&self) -> bool {
        self.quantity <= 0.0
    }
}

/// Market data event enum
#[derive(Debug, Clone)]
pub enum MarketDataEvent {
    OrderBookUpdate(OrderBookUpdate),
    AggTrade(AggTrade),
    MarkPriceUpdate(MarkPriceUpdate),
    ForceOrder(ForceOrder),
    MarketDataHalt,
}