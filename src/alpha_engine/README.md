# Project AEGIS - Layer 2
## Alpha Generation & Signal Logic Engine

### 🏛️ Official Reference Document: Layer 2 (Project AEGIS - Layer 2)
**Classification:** Secret / Quantitative Systems Engineering
**Unit:** Signal Generation Engine & Alpha Logic

---

## 🔧 Architectural Philosophy of Alpha

The system does not rely on binary indicators (true/false), but instead uses a "Weighted Confluence Scoring System". Each trading signal is a mathematical result of several precise market microstructure factors. The "Net Expected Value" (NEV) must exceed the zero threshold after deducting the worst-case scenario for fees and slippage.

---

## 📊 Core Microstructure Features

These are the raw data transformed by the engine into signals. They are calculated in-memory with minimal latency:

### 2.1. Order Book Imbalance (OBI)
**Logic:** Measures instantaneous pressure between buy and sell orders at the top 5 price levels.
**Formula:** `OBI = (Sum(Bid_Volumes) - Sum(Ask_Volumes)) / (Sum(Bid_Volumes) + Sum(Ask_Volumes))`
**Usage:** If OBI approaches +1 (massive buying pressure) and price is near a support level, the likelihood of upward bounce increases.

### 2.2. Trade Flow Toxicity (VPIN)
**Logic:** Detects whether "informed traders" (whales) are placing market orders to consume liquidity.
**Design:** Aggregates executed trades (AggTrades) into "volume buckets" instead of time buckets. If 80% of volume in the last bucket consists of taker sells, toxicity is high.
**Usage:** Defensive indicator. If toxicity is high against our trade direction, entry orders are canceled immediately.

### 2.3. Liquidity Voids & Spread Dynamics
**Logic:** Markets move toward "voids" (areas of liquidity absence in the order book) because they require less effort to penetrate.
**Design:** Algorithm detects price gaps larger than 3 * Tick Size with no large limit orders.

### 2.4. Open Interest Delta (OI Δ)
**Logic:** In perpetual contracts, price alone is insufficient. Rising price with falling OI indicates "short squeeze" (false breakout). Rising price with rising OI confirms trend.

---

## 🧠 Signal Logic Matrix

The engine operates as a "Finite State Machine" (FSM) for each symbol:

1. **Idle State:** Monitors OBI and liquidity. No orders.
2. **Armed State:** Entry conditions met (e.g., OBI > 0.4 + price at high liquidity level). Bot calculates exact entry price and prepares order.
3. **Executing State:** Sends Post-Only Limit Order to Layer 3.
4. **Managing State:** Position open. Engine monitors VPIN and OBI for early exit if market structure flips, or lets price reach take-profit.

---

## 💰 Dynamic Capital Adaptation Engine

Responding to the fundamental requirement: Absolute adaptation to any capital size.

The bot doesn't know the concept of "fixed position size". It only knows "risk percentage".

**Mathematical Engine (Volatility-Adjusted Sizing):**
1. Fetch actual capital: Read Cross/Isolated Wallet Balance in real-time.
2. Calculate absolute risk: `Risk_Amount = Equity * 0.01` (e.g., 1% of balance).
3. Determine stop distance: Based on instantaneous volatility (Micro-ATR) or last bottom/top in order book.
4. Position size: `Size = Risk_Amount / Stop_Distance`.

**Engineering Protection (Boundary Safeguards):**
If `Size * Entry_Price` is less than platform minimum (e.g., 5 USDT on Binance):
- **Option A (Safer):** Ignore signal (Skip Signal) and log "Capital Too Small for Volatility".
- **Option B (Aggressive Adaptation):** Adjust Stop_Distance to be closer, increasing Size to meet 5 USDT requirement, provided stop loss isn't too tight.

---

## ⚠️ Failure & Success Analysis

Applying the "Double Lens" for survival and profitability:

### ⚠️ Failure Scenario 1: Fee & Slippage Bleed
**Problem:** Bot wins 55% of trades but loses money overall. Cause: Entering/exiting with market orders (taker) pays fees (0.04% + 0.04%) plus price slippage (~0.02%). Total cost 0.1% per trade, erasing scalping profits.
**Radical Hardening (Maker-First Architecture):**
- Entry: Only limit orders with Post-Only flag. Ensures liquidity making (maker) with lower fees/discounts and no slippage.
- Emergency Exit: Market orders (taker) only if "flow toxicity" suddenly flips against us.
- Net Expectancy Rule: No signal activates unless: `(Target Profit - Entry Fee - Exit Fee - Estimated Slippage) > 0`.

### ⚠️ Failure Scenario 2: Adverse Selection / Toxic Fill
**Problem:** Place buy limit order. Suddenly, whale sells massive quantity with market order. Your order fills completely, then price crashes instantly. You bought the exact top before collapse.
**Radical Hardening (Queue Position & Toxicity Monitor):**
- Pre-trade cancel: If your buy order waits in book, and engine detects toxic sell flow approaching your price, bot sends cancel order before whale reaches your price.
- Tail monitoring: Don't place orders at best price (top of book) to avoid direct collision. Retreat one or two ticks to absorb first shock.

### ⚠️ Failure Scenario 3: Overfitting the Backtest
**Problem:** Alpha logic works 90% on historical data but fails immediately in live market because historical data lacks "network latency" or "your orders' market impact".
**Radical Hardening (Walk-Forward & Realistic Simulation):**
- Engine must include internal "execution simulator" adding random latency (5-15 ms) per trade in testing.
- No alpha logic published unless it passes "out-of-sample" data with full taker fees (worst-case scenario).

### ⚠️ Failure Scenario 4: Logic Paralysis from Complexity
**Problem:** Calculating complex indicators (VPIN + OBI together) takes 50ms CPU time. Meanwhile, market moves and signal is missed.
**Radical Hardening (Lock-Free Incremental Calculation):**
- Indicators not recalculated from scratch on each tick.
- Use "incremental updates". When order book update arrives, only subtract old volume and add new volume to OBI equation. Reduces computation time from O(N) to O(1) (microseconds).

---

## 🔗 Integration with Other Layers

**Receiving:** Reads from Layer 1's Ring Buffer without waiting (non-blocking).
**Sending:** When generating signal, wraps it in TradeIntent object containing: `[Symbol, Side, Price, Size, StopLoss, TakeProfit, TimeToLive]` and sends via crossbeam channel to Layer 3.
**Signal Validity (Time-To-Live - TTL):** Each signal has virtual lifespan (e.g., 200ms). If Layer 3 doesn't execute within this time, it's canceled. In Hyper-Scalping, old signal = toxic signal.

---

## 🚀 Getting Started

### Prerequisites
- Rust 1.65+
- Cargo

### Building
```bash
cd src/alpha_engine
cargo build --release
```

### Running
```bash
cd src/alpha_engine
cargo run --release
```

---

## 📊 Performance Targets

- **Latency:** < 20 microseconds for signal generation
- **Throughput:** > 50,000 signals per second
- **Memory Usage:** < 50MB baseline
- **Signal Accuracy:** > 55% win rate with positive expectancy

---

*This system is for educational purposes only. Trading cryptocurrencies involves substantial risk.*