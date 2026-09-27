# Project AEGIS - Layer 4
## Telemetry, Risk Management & Circuit Breakers

### 🏛️ Official Reference Document: Layer 4 (Project AEGIS - Layer 4)
**Classification:** Secret / Survival & Quantitative Risk Engineering
**Unit:** Telemetry, Risk Management & Circuit Breakers

---

## 🔧 Architectural Philosophy of Risk

The system is designed to be "Anti-Martingale". We don't double position size after a loss to recover it (that's the path to certain bankruptcy). We reduce exposure when conditions deteriorate, and gradually increase it only when the system proves its statistical efficiency (Positive Expectancy) on current capital.

Monitoring must be entirely asynchronous; no logging or risk calculation process is allowed to slow down the "hot path" of order execution in Layer 3.

---

## 🧩 Core Components Architecture

### 4.1. Global Risk Manager (GRM)
Operates as an independent thread reading wallet state and open positions in real-time.

**Absolute Capital Adaptation:**
GRM calculates risk amount for the next trade based on actual available equity, not initial balance.
**Governing Equation:** `Risk Amount = Live Equity * Risk %`. If portfolio loses 10%, next trade size automatically decreases by 10%. This ensures zero chance of reaching zero mathematically (Asymptotic to Zero).

**Maximum Adverse/Favorable Excursion Tracking (MAE/MFE):**
- **MAE (Maximum Adverse Excursion):** Maximum unrealized loss a trade reached before closing
- **MFE (Maximum Favorable Excursion):** Maximum unrealized profit a trade reached
**Purpose:** If MAE consistently breaches stop loss level, it indicates Layer 2 entry logic is lagging and needs adjustment.

### 4.2. Hierarchical Circuit Breakers
Multi-level defensive system automatically triggered upon anomaly detection:

**Level 1 (Warning - Yellow Alert):**
- **Trigger:** Latency with Binance > 50ms, or Order Rejection Rate > 5%
- **Action:** Reduce new trade size by 50%, cancel distant entry orders

**Level 2 (Entry Halt - Orange Halt):**
- **Trigger:** Daily Drawdown reaches 2% of total capital, or persistent "toxic flow" in VPIN
- **Action:** Immediately halt opening new trades. Only allow managing/closing open positions

**Level 3 (Full Kill - Red Kill-Switch):**
- **Trigger:** Flash crash (price moves >3% in <10 seconds), or USER_DATA_STREAM disconnect > 5 seconds, or weekly loss reaches 5%
- **Action:** Emergency DELETE /fapi/v1/allOpenOrders followed by MARKET orders to close all positions. Complete system shutdown and immediate engineer alert

### 4.3. Market Regime & Funding Guard
**Funding Rate Sentinel:** In perpetual contracts, if funding rate is very negative (everyone is selling), going LONG exposes you to massive funding fees every 8 hours. Guard prevents opening LONG positions if Funding Rate < -0.05% unless mathematical expectancy exceeds this cost by 3x.

**Volatility Anomaly Detector:** Measures standard deviation of price movement (Tick Velocity) over 10-second window. If volatility jumps 400% above average (indicator of surprise news or manipulation), Level 2 circuit breaker activates immediately.

### 4.4. Telemetry & Observability Stack
**Engineering Design:** Uses Rust tracing library with asynchronous appender pushing metrics via UDP to local Prometheus server. Ensures zero-blocking I/O for logging.

**Grafana Dashboard** displays real-time:
- **Live Latency:** Binance response time (Tick-to-Trade)
- **Actual Exposure:** Open position size in USDT
- **Win Rate & Slippage:** Difference between theoretical signal price and actual execution price
- **Equity Curve:** Comparison between net profits (after fees) and gross profits

---

## ⚠️ Failure & Success Analysis

Applying "Double Lens" for disaster scenarios:

### ⚠️ Failure Scenario 1: "Silent Bleed"
**Problem:** Bot opens/closes hundreds of trades daily. Win rate 52%. Seems successful, but weekend portfolio loses 4%. Cause: Bot doesn't accurately calculate "actual slippage" and "fees" in strategy performance evaluation.
**Radical Hardening (Realized vs. Unrealized Alpha Tracking):**
GRM calculates Net Expected Value for each trade after closure: `(Exit Price - Entry Price) - (Taker Fees * 2) - Slippage`.
If "Rolling Net EV" for last 100 trades drops below zero, Level 2 circuit breaker activates automatically. Bot shuts itself down before draining portfolio.

### ⚠️ Failure Scenario 2: Drawdown Cascade / Tilt
**Problem:** Bot loses 3 consecutive trades due to unfavorable market conditions. Bad algorithm might "revenge trade" by increasing leverage to recover loss, leading to liquidation.
**Radical Hardening (Consecutive Loss Dampener):**
Apply "Consecutive Loss Rule": After each consecutive loss, multiply Risk % by 0.8 factor. (Example: 1% -> 0.8% -> 0.64%).
Risk size only returns to 1% after achieving 3 consecutive winning trades. This strongly protects capital during "strategy inefficiency periods".

### ⚠️ Failure Scenario 3: Telemetry Overhead Paralysis
**Problem:** Logging every order book tick to file consumes 100% CPU (I/O bottleneck), causing missed trading signals.
**Radical Hardening (Sampling & Lock-Free Logging):**
No raw data logging: WebSocket streams aren't saved to live logs. Only "signals" and "execution events" are recorded.
Sampling: Metrics like OBI/VPIN sent to Prometheus once every 100ms (Downsampling) to reduce memory/network load, while maintaining microsecond accuracy in internal execution loop.

### ⚠️ Failure Scenario 4: Price Manipulation (Wick Hunting)
**Problem:** Whales violently push price down for one second (Shadow/Wick) to trigger stop-loss orders, then price rebounds. Bot loses due to market "noise".
**Radical Hardening (Time-Weighted Stop Logic):**
Instead of placing STOP_MARKET at fixed price only, bot (via Layer 2) monitors price.
If price breaches stop level, closure only happens if price stabilizes below stop level for > 200ms, or breach accompanied by "volume spike". This ignores flash wicks. (Note: Requires very precise balance to avoid missing real crashes, so only used in very high liquidity markets.)

---

## 🔁 Recovery & Resumption Protocol

When circuit breaker halts bot, how does it resume?

**Cooldown:** After any emergency shutdown, system enters mandatory idle state for 15 minutes.

**Health Check:** Verifies WebSocket connection, local order book sync, funding rate stability.

**Recalibration:** Recalculates Live Equity and adjusts Risk % based on new balance.

**Cautious Resumption:** Begins trading at 25% normal size. If first 5 trades succeed, gradually returns to 100%.

---

## 🚀 Getting Started

### Prerequisites
- Rust 1.65+
- Cargo
- Prometheus & Grafana (for telemetry)

### Building
```bash
cd src/risk_management
cargo build --release
```

### Running
```bash
cd src/risk_management
cargo run --release
```

---

## 📊 Performance Targets

- **Latency:** Zero impact on trading execution path
- **Reliability:** 99.999% uptime with automatic recovery
- **Risk Control:** Maximum 5% weekly drawdown
- **Alerting:** < 1 second detection of anomalies

---

*This system is for educational purposes only. Trading cryptocurrencies involves substantial risk.*