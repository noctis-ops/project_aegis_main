//! WebSocket connection handler for Binance Futures

use crate::core::{HftError, MarketDataEvent, OrderBookUpdate, AggTrade, MarkPriceUpdate, ForceOrder};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use url::Url;
use tracing::{info, error, warn};
use tokio::sync::mpsc;
use crate::core::constants::*;

/// WebSocket connection manager for a specific symbol
pub struct WebSocketManager {
    symbol: String,
    tx: mpsc::UnboundedSender<MarketDataEvent>,
}

impl WebSocketManager {
    /// Create a new WebSocket manager for a symbol
    pub fn new(symbol: String, tx: mpsc::UnboundedSender<MarketDataEvent>) -> Self {
        Self { symbol, tx }
    }
    
    /// Start the WebSocket connection and begin processing messages
    pub async fn start(&self) -> Result<(), HftError> {
        let streams = vec![
            format!("{}@depth@0ms", self.symbol.to_lowercase()),
            format!("{}@aggTrade", self.symbol.to_lowercase()),
            format!("{}@markPrice@1s", self.symbol.to_lowercase()),
            format!("{}@forceOrder", self.symbol.to_lowercase()),
        ];
        
        let stream_param = streams.join("/");
        let ws_url = format!("{}{}", BINANCE_FUTURES_WS_URL, stream_param);
        
        info!("Connecting to WebSocket: {}", ws_url);
        
        let (ws_stream, _) = connect_async(Url::parse(&ws_url)?).await?;
        let (mut write, mut read) = ws_stream.split();
        
        // Spawn a task to handle incoming messages
        let tx = self.tx.clone();
        let symbol = self.symbol.clone();
        // Precomputed once per connection instead of allocating a new
        // uppercase String for every single inbound message.
        let symbol_upper = self.symbol.to_uppercase();
        
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = Self::handle_message(text, &tx, &symbol_upper).await {
                            error!("Error handling WebSocket message: {}", e);
                        }
                    }
                    Ok(Message::Ping(ping)) => {
                        if let Err(e) = write.send(Message::Pong(ping)).await {
                            error!("Error sending pong: {}", e);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        warn!("WebSocket connection closed for symbol: {}", symbol);
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error for symbol {}: {}", symbol, e);
                        break;
                    }
                    _ => {}
                }
            }
        });
        
        Ok(())
    }
    
    /// Handle incoming WebSocket messages and convert them to internal events
    async fn handle_message(
        message: String,
        tx: &mpsc::UnboundedSender<MarketDataEvent>,
        symbol_upper: &str,
    ) -> Result<(), HftError> {
        // Try to parse as order book update first
        if message.contains("\"e\":\"depthUpdate\"") {
            // `into_bytes()` moves the message buffer into simd-json with
            // zero copying (`as_bytes().to_vec()` allocated and copied the
            // whole payload on every single message).
            let mut data = message.into_bytes();
            let update: OrderBookUpdate = simd_json::from_slice(&mut data)?;
            
            if update.symbol == symbol_upper {
                tx.send(MarketDataEvent::OrderBookUpdate(update))
                    .map_err(|_| HftError::Other("Failed to send order book update".to_string()))?;
            }
        }
        // Try to parse as aggregated trade
        else if message.contains("\"e\":\"aggTrade\"") {
            let mut data = message.into_bytes();
            let trade: AggTrade = simd_json::from_slice(&mut data)?;
            
            if trade.symbol == symbol_upper {
                tx.send(MarketDataEvent::AggTrade(trade))
                    .map_err(|_| HftError::Other("Failed to send agg trade".to_string()))?;
            }
        }
        // Try to parse as mark price update
        else if message.contains("\"e\":\"markPriceUpdate\"") {
            let mut data = message.into_bytes();
            let mark_price: MarkPriceUpdate = simd_json::from_slice(&mut data)?;
            
            if mark_price.symbol == symbol_upper {
                tx.send(MarketDataEvent::MarkPriceUpdate(mark_price))
                    .map_err(|_| HftError::Other("Failed to send mark price update".to_string()))?;
            }
        }
        // Try to parse as force order
        else if message.contains("\"e\":\"forceOrder\"") {
            let mut data = message.into_bytes();
            let force_order: ForceOrder = simd_json::from_slice(&mut data)?;
            
            if force_order.order.symbol == symbol_upper {
                tx.send(MarketDataEvent::ForceOrder(force_order))
                    .map_err(|_| HftError::Other("Failed to send force order".to_string()))?;
            }
        }
        
        Ok(())
    }
}
