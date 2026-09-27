//! Interface for communication with other layers

use crossbeam::channel::{Receiver, Sender};
use crate::core::{TradeIntent, ExecutionReport};

/// Layer interface for inter-component communication
pub struct LayerInterface {
    trade_intent_rx: Option<Receiver<TradeIntent>>,
    execution_report_tx: Option<Sender<ExecutionReport>>,
}

impl LayerInterface {
    /// Create a new layer interface
    pub fn new() -> Self {
        Self {
            trade_intent_rx: None,
            execution_report_tx: None,
        }
    }
    
    /// Set trade intent receiver from Layer 2
    pub fn set_trade_intent_receiver(&mut self, rx: Receiver<TradeIntent>) {
        self.trade_intent_rx = Some(rx);
    }
    
    /// Set execution report sender to Layer 2/4
    pub fn set_execution_report_sender(&mut self, tx: Sender<ExecutionReport>) {
        self.execution_report_tx = Some(tx);
    }
    
    /// Receive trade intent (non-blocking)
    pub fn receive_trade_intent(&self) -> Option<TradeIntent> {
        if let Some(rx) = &self.trade_intent_rx {
            match rx.try_recv() {
                Ok(intent) => Some(intent),
                Err(crossbeam::channel::TryRecvError::Empty) => None,
                Err(crossbeam::channel::TryRecvError::Disconnected) => {
                    tracing::error!("Trade intent channel disconnected");
                    None
                }
            }
        } else {
            None
        }
    }
    
    /// Send execution report
    pub fn send_execution_report(&self, report: ExecutionReport) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = &self.execution_report_tx {
            match tx.send(report) {
                Ok(_) => {
                    tracing::debug!("Execution report sent");
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("Failed to send execution report: {}", e);
                    Err(Box::new(e))
                }
            }
        } else {
            Err("Execution report sender not set".into())
        }
    }
    
    /// Check if there are pending trade intents
    pub fn has_pending_trade_intents(&self) -> bool {
        if let Some(rx) = &self.trade_intent_rx {
            !rx.is_empty()
        } else {
            false
        }
    }
}