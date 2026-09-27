# Project AEGIS - Layer 3
## Order Management System & Execution Engine

### 🏛️ Official Reference Document: Layer 3 (Project AEGIS - Layer 3)
**Classification:** Secret / HFT Execution Engineering
**Unit:** Order Management System & Execution Engine

---

## 🔧 Architectural Philosophy of Execution

Execution is not merely sending a POST /order request. It is a complex "Finite State Machine" that manages the order lifecycle from birth (TradeIntent) to death (Filled, Canceled, or Rejected).

The golden rule here: There is no "unknown state". The bot must know exactly what its actual exposure is on Binance servers at every millisecond.

---

## 🧩 Core Components Architecture

### 3.1. Smart Order Router (SOR)
Decides how and where to send orders based on Layer 2 intent and market conditions:

**Entry Orders:** Sent exclusively as limit orders with timeInForce=GTX property
**Emergency Exits:** Sent as market orders or immediate-cancel limit orders at distant prices for instant execution (Taker)
**Communication Protocol:** Uses Binance WebSocket Order API (/ws-fapi/v1) instead of REST API to reduce latency and avoid HTTP bottlenecks. REST API is used only as fallback.

### 3.2. Order Lifecycle Manager (OLM)
Tracks each order's state using a unique identifier (clientOrderId in UUIDv4 format) to prevent duplication (Idempotency).

**Tracked States:** NEW -> PARTIALLY_FILLED -> FILLED -> CANCELED -> EXPIRED -> REJECTED
**Partial Fill Handler:** In Hyper-Scalping, partial fills are a nightmare. If we requested 100 contracts and only 20 executed, and Layer 2's signal flipped to "sell", OLM must immediately cancel the remaining 80 and close the 20 (reverse direction).

### 3.3. State Reconciliation Engine
**Reception:** Listens to Binance's USER_DATA_STREAM (WebSocket) to receive real-time execution reports and balance updates.
**Engineering Design:** Uses a super-fast concurrent map (like DashMap in Rust) to store active orders. When an executionReport message arrives, the state is updated in memory with near-zero latency (lock-free reads).

### 3.4. Dynamic Margin & Leverage Guard
**Capital Integration:** Receives Position Size from Layer 2. Before sending orders, it checks Binance's strict filters:
- MIN_NOTIONAL: (Size * Price) must be > 5 USDT
- LOT_SIZE: Precise rounding of Quantity to match platform step size
**Leverage Management:** Doesn't change leverage per trade (causes delay and extra API calls). Fixes leverage at maximum allowed (e.g., 50x or 100x) and uses isolated margin, then mathematically controls "actual position size". High leverage here is just a tool to reduce locked margin, not to amplify risk.

---

## ⚠️ Failure & Success Analysis

Applying the "Double Lens" for portfolio protection from execution disasters:

### ⚠️ Failure Scenario 1: The Orphaned Order Nightmare
**Problem:** Bot sends buy order. Bot's WebSocket connection drops for seconds. Meanwhile, order partially executes then market crashes violently. Bot is "blind" and doesn't know it has an open position suffering massive losses.
**Radical Hardening (Kill-Switch & Reconciliation):**
- Heartbeat: If USER_DATA_STREAM disconnects for more than 3 seconds, "emergency state" activates
- Panic Button: Bot immediately sends REST API request: DELETE /fapi/v1/allOpenOrders
- Ground Truth Fetching: Queries actual open positions via GET /fapi/v2/positionRisk. If any open position doesn't match local memory, immediately closes it with market order regardless of profit/loss. Survival first.

### ⚠️ Failure Scenario 2: Rate Limit Ban (HTTP 429/418)
**Problem:** During volatile markets, bot cancels and reprices orders dozens of times per second. Exceeds Binance limits (e.g., 10 requests/sec or 2400 weight/min), leading to IP ban for days.
**Radical Hardening (Local Token Bucket & Weight Tracking):**
- Strict local rate limiter mimicking Binance rules exactly
- Each request has "weight". If remaining weight < 10%, OLM refuses non-critical orders (entries) and allows only emergency exits
- Reads x-mbx-used-weight-1m response headers from Binance for real-time local limiter updates

### ⚠️ Failure Scenario 3: Flash Crash Liquidation
**Problem:** Bot relies on "software stop loss" (Soft Stop) in Layer 2. Sudden flash crash occurs. Bot delays or internet cuts, exit order never sends. Account fully liquidated.
**Radical Hardening (Server-Side Hard Stops):**
- Mandatory Rule: No open trade without stop loss on Binance servers
- Bracket Orders: Once entry order fills (confirmed via USER_DATA_STREAM), OLM immediately and asynchronously sends STOP_MARKET (stop loss) and TAKE_PROFIT_MARKET (profit taking) to Binance
- If position closes manually or via reverse signal, must first cancel old STOP_MARKET and TAKE_PROFIT orders before sending new ones

### ⚠️ Failure Scenario 4: Slippage on Exit (Exit Slippage Disaster)
**Problem:** Bot uses market orders to exit. During low liquidity moments, order executes at much worse prices than expected, turning theoretically profitable trade into actual loss.
**Radical Hardening (Smart Exit Routing):**
- Instead of traditional MARKET, bot uses LIMIT orders with timeInForce=IOC property and slightly worse price (e.g., 2-3 Ticks)
- Ensures instant execution as Taker, but protects against extreme slippage. If liquidity is so poor that price slips more than 3 Ticks, unfilled portion cancels instead of buying at catastrophic prices

---

## 🔗 Data & Integration Architecture

**Ultra-Fast Zero-Allocation HMAC-SHA256 Signature:**
Binance order signing requires HMAC-SHA256 calculation. In Rust, uses ring or sha2 with hmac libraries.
**Hardening:** Pre-allocates buffers at startup to build query strings and calculate signatures, avoiding any heap allocation in hot path when sending orders.

**Error Handling Strategy:**
- If Binance replies {"code": -2019, "msg": "Margin is insufficient"}, OLM immediately sends event to Layer 4 (Risk Manager) to reduce risk percentage in future trades by 50%
- If reply is {"code": -5022, "msg": "Post-Only orders will be rejected"} (due to market volatility entering as Taker by mistake), OLM cancels order immediately and notifies Layer 2 to adjust price

---

## 💰 Dynamic Capital Adaptation Engine

To ensure absolute adaptation to any capital size:

**Pre-Trade Notional Check:** Before sending any order, OLM calculates: Notional = Price * Quantity
**Constraint Adaptation:**
- If Notional < MIN_NOTIONAL (e.g., 5 USDT), OLM rejects execution and sends OrderRejected_CapitalTooLow event
- If account uses isolated leverage and capital is very small, OLM ensures Quantity * Price / Leverage doesn't exceed available balance (Available Balance) after deducting expected fees

---

## 🚀 Getting Started

### Prerequisites
- Rust 1.65+
- Cargo
- Binance API credentials (for production use)

### Building
```bash
cd src/execution_engine
cargo build --release
```

### Running
```bash
cd src/execution_engine
cargo run --release
```

---

## 📊 Performance Targets

- **Latency:** < 10 microseconds for order placement
- **Throughput:** > 100,000 orders per second
- **Memory Usage:** < 75MB baseline
- **Reliability:** 99.99% order execution success rate

---

*This system is for educational purposes only. Trading cryptocurrencies involves substantial risk.*