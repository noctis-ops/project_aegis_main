# Project AEGIS - Layer 5
## Backtesting Infrastructure, Simulation & CI/CD Deployment

### 🏛️ Official Reference Document: Layer 5 (Project AEGIS - Layer 5)
**Classification:** Secret / Simulation & Cloud Infrastructure Engineering
**Unit:** Backtesting Infrastructure, Simulation & CI/CD Deployment

---

## 🔧 Architectural Philosophy

**Deterministic Path Parity:** The code that reads historical data in simulation must be exactly the same code (same structs, same channels) that reads live WebSocket data. We don't write "test code" and "production code".

**Financial Pessimism:** Simulation always assumes worst-case slippage scenario, places your orders at the back of the queue, and calculates maximum possible fees. If the strategy succeeds here, it's truly profitable in real markets.

**Infrastructure as Code (IaC):** Manual deployment is forbidden. Everything from server provisioning to firewall rules is programmatically defined for reproducible reliability.

---

## 🧩 Core Components Architecture

### 5.1. Tick-Level Data Lake
For Hyper-Scalping testing, we need microsecond-accurate historical data.

**Storage Format:** Apache Parquet columnar format. Allows reading millions of rows in seconds thanks to high compression and indexing.

**Data Content:**
- **L2_Orderbook_Snapshots:** Order book snapshots every 100 milliseconds
- **Tick_Trades:** Every executed trade (Price, Quantity, Side, Timestamp)

**Data Feeder:** Software unit that reads Parquet files and injects messages into Tokio channels (same channels as Layer 1) with controlled injection speed (Playback Speed).

**Data Requirement (as of the real replay):** every gate that reports backtest numbers -
`SimulationEngine::run_backtest`, the robustness and overfitting sweeps, and the CI smoke test -
reads recorded events from `data_path` (default `./data`, relative to the working directory).
Accepted files are `*.parquet`, `*.jsonl` and `*.json`; a row is either a serialized `MarketEvent`
(`{"Trade": {...}}`) or a flat record:

| row | fields |
|-----|--------|
| order book snapshot | `symbol`, `timestamp`, and `bids`/`asks` (an array of `{price, quantity}`) or the paired columns `bid_price_0`, `bid_qty_0`, `ask_price_0`, `ask_qty_0`, ... |
| trade | `symbol`, `timestamp`, `price`, `quantity`, and `side` (`"buy"`/`"sell"`) or `is_buyer_maker` |
| funding | `symbol`, `timestamp`, `funding_rate` (or `rate`) |

Timestamps are epoch **milliseconds** (or an RFC 3339 string). If the lake holds no files - or no
rows inside the requested window - the run fails with a `Data loading error` instead of reporting
numbers. That is deliberate: this layer used to manufacture two events dated 2022-01-01 and a fixed
"12% return / 800 trades" report, which made every gate above it unable to say no.

### 5.2. Event-Driven Backtesting Engine
**Engineering Design:** Engine operates as a central Event Loop. Receives MarketEvent from Data Feeder, passes it to Layer 2 (Alpha), then receives TradeIntent and passes it to Execution Simulator.

**Dynamic Simulation Adaptation:** Engine is fed with "Virtual Equity". Engine applies Layer 4 equations (Volatility-Adjusted Sizing) precisely. We test the system with $100 capital, then $10,000, then $1,000,000 to see how performance is affected by "Market Impact" at large sizes.

### 5.3. Pessimistic Execution Simulator
This is the heart of financial realism. Simulates your orders' interaction with historical market:

**Queue Position Modeler:** If bot sends limit buy order at price 100, it's not executed immediately. Simulation calculates "cumulative volume" ahead of your order in L2 queue. Your order executes only if historical Tick_Trades data shows sell volume at price 100 exceeded queue volume ahead of you.

**Latency Simulator:** Adds random jitter between 2 to 10 milliseconds between signal issuance and arrival at simulator, mimicking real AWS-Binance network reality.

### 5.4. Shadow Trading Infrastructure
**Design Architecture (Polymorphic Execution):**
Shared "Execution Trait" interface designed for polymorphic execution engine.
System decides at runtime whether to use LiveExecutionEngine (connected to Binance API) or ShadowExecutionEngine (local simulation engine).

**Pessimistic Local Matching Engine:**
Receives TradeIntent from Layer 2.
Reads live L2_Orderbook data from Layer 1.
Strict Execution Rule: Virtual buy/sell orders execute only if live market data proves price moved through our level and cumulative volume suffices to fill queue ahead.

**Virtual PnL Tracking:** Calculates virtual profits/losses after deducting real Binance fees (Maker/Taker) and estimating slippage, sending reports to Telegram interface.

### 5.5. Telegram C2 Microservice
**Arabic Remote Control Interface:** Engineered as "Command-Only" communication channel for maximum security.

**Async Non-Blocking Architecture:**
Telegram bot operates as independent Tokio Task in background.
Bot communicates with Layer 4 Risk Manager and Layer 3 Order Manager via ultra-fast MPSC channels.
Does not connect directly to Binance databases.

**Zero-Trust C2 Security:**
Strict Whitelisting: Bot programmed to reject any message not bearing engineer/founder's Telegram User ID.
Key Isolation: Component contains no Binance API keys. Merely a "signal transmitter."

**Arabic Control Panel (Functional Specifications):**
**Commands:**
- `/status`: Displays (live balance, current exposure, daily PnL, connection status)
- `/halt`: Immediately activates "Red Kill-Switch": cancels all orders, flattens positions, stops engine
- `/resume`: Resumes work after system self-check
- `/risk [percentage]`: Modifies dynamic risk coefficient (e.g., `/risk 0.5` to reduce risk to 0.5%)
- `/mode [shadow/live]`: Switches between shadow trading and live trading modes (requires safe restart)

  The code `/halt` and `/mode live` demand is issued by `SecurityValidator::issue_confirmation_code`:
  random, single use, valid for 15 minutes, printed once in the log at issue time. It used to be
  `AEGIS{YYYYMMDD}` - computable in advance by anyone who knew the format, for the one credential
  that hands the bot real money.

**Instant Notifications (Push Telemetry):**
Bot sends automatic Arabic messages for: (trade opening, trade closing, circuit breaker activation, connection errors)

### 5.6. Automated CI/CD Pipeline
**Hosting:** AWS EC2 c6i.xlarge compute-optimized server in ap-northeast-1 (Tokyo) region for lowest latency to Binance servers.

**Containerization:** Docker with distroless/alpine ultra-light image to minimize attack surface.

**GitHub Actions Automation:**
- **Push:** On code push, runs unit tests and quick smoke backtest
- **Build:** Compiles Rust code in --release mode with native CPU optimizations
- **Deploy:** Pushes image to AWS ECR, then updates server via AWS ECS or Systemd

---

## ⚠️ Failure & Success Analysis

Applying "Double Lens" to simulation and deployment disasters:

### ⚠️ Failure Scenario 1: "Future Peek" Illusion (Look-Ahead Bias)
**Problem:** In simulation, bot reads current candle's closing price and decides to enter based on it. In reality, this price wasn't known until candle end. This generates phantom millions in testing, but immediate losses in production.

**Radical Hardening (Strict Chronological Injection):**
Data Feeder is engineering-constrained by "Strict Timestamp". Layer 2 (Alpha) is forbidden from accessing any data bearing timestamp greater than Current_Simulation_Time.
Orders issued at time T cannot interact with market data until time T + Latency.

### ⚠️ Failure Scenario 2: Shadow Trading Over-Optimism
**Problem:** Local engine executes virtual orders immediately on price touch. In reality, your orders will be at queue end and may not execute. This gives false confidence before risking real money.

**Radical Hardening (Queue Position Modeling):**
As designed in (5.4), shadow engine must be "pessimistic". If volume ahead in real order book is 100 BTC, engine must record real market sales of at least 100 BTC before considering virtual order executed.

### ⚠️ Failure Scenario 3: C2 Thread Contention
**Problem:** If internet disconnects or Telegram servers delay, C2 Thread may block, causing "freezing" of main bot engine and missing critical trading signals.

**Radical Hardening (Fire-and-Forget Telemetry):**
Notifications sent to crossbeam unbounded channel.
C2 Microservice reads from this channel and sends Telegram messages. If sending fails, stored in local retry queue without ANY impact on trading hot path.

### ⚠️ Failure Scenario 4: Social Engineering Compromise
**Problem:** Telegram account theft grants attacker ability to send /halt or modify risk settings to sabotage bot.

**Radical Hardening (Multi-Factor Command Validation):**
Sensitive commands (like /halt or /mode live) require double confirmation.
When sending /halt, bot replies: "⚠️ Are you sure to halt system and close positions? Send secret code to confirm." (Code generated daily and stored in separate secure environment).

### ⚠️ Failure Scenario 2: "Instant Fill" Illusion (Phantom Fills)
**Problem:** Simulation assumes that once price touches your limit order price, trade executes. In Hyper-Scalping, this is false. Price may touch your level for a fraction of a second and retreat, with no execution because you were at queue end.

**Radical Hardening (Volume-Through Fill Logic):**
As mentioned in (Queue Position Modeler), Touch ≠ Fill. Execution requires liquidity depletion ahead of your order. This reduces win rate in simulation by ~30%, but gives you "True Expectancy".

### ⚠️ Failure Scenario 3: Curve Overfitting Catastrophe
**Problem:** Tuning entry criteria (like OBI > 0.45) to work perfectly on 2023 data. When live in 2024, bot collapses because "Market Regime" changed.

**Radical Hardening (Walk-Forward Optimization & Out-of-Sample Testing):**
Data split into two windows: 70% for training (In-Sample) and 30% for blind testing (Out-of-Sample).
Robustness Testing: If strategy profits at OBI > 0.45, it must also profit (even if less) at 0.40 and 0.50. If performance collapses with tiny changes, model is "overfitted" and must be rejected.

### ⚠️ Failure Scenario 4: Secret Leak & Downtime Disaster
**Problem:** Storing Binance API keys in .env file accidentally pushed to GitHub, or server crashing during open trade due to manual update.

**Radical Hardening (Zero-Trust IAM & Auto-Recovery):**
Keys: No keys in code. Injected at runtime via AWS Secrets Manager.
Permissions: API key has only Enable Futures Trading permission, with IP restriction (Whitelist) allowing only our EC2 IP. Even if key stolen, unusable from outside server.
Recovery: systemd with Restart=always policy. If bot panics, restarts in <1 second. On startup, executes Layer 3 "Ground Truth Fetching" protocol to reconcile local state with actual open positions on Binance.

---

## 💰 Capital Adaptation Simulation

To prove fulfillment of core requirement (absolute adaptation to any capital size):

"Test Matrix" runs automatically in CI/CD:
- **Path A:** $100 capital, isolated leverage. (Simulation verifies MIN_NOTIONAL compliance and avoids rapid liquidation).
- **Path B:** $50,000 capital. (Verifies Volatility-Adjusted Sizing calculation efficiency and order book liquidity compliance).
- **Path C:** $2,000,000 capital. (Here most bots fail). Simulation enforces "Market Impact"; i.e., large buy orders will push price against you (Slippage). Engine must prove ability to split orders (TWAP/VWAP logic) if needed.

---

## 🏁 Final Summary for Chief Engineer (Project AEGIS - Complete)

Layer 5 design completed with Shadow Trading & Telegram C2 enhancements.
Together we have built an "engineering and financial constitution" for a professional-grade HFT bot:

- **Layer 1:** Ultra-fast nervous system (Rust, WebSockets, Lock-Free LOB)
- **Layer 2:** Quantum brain exploiting market physics (OBI, VPIN, Net Expectancy)
- **Layer 3:** Surgical execution arm with orphan protection (Smart Routing, Server-Side Stops)
- **Layer 4:** Ruthless immune system (Circuit Breakers, Dynamic Capital Sizing, Anti-Martingale)
- **Layer 5:** Truth gate and infrastructure (Pessimistic Shadow Trading, Telegram C2, CI/CD, Colocation)

This design doesn't promise fantasy profits, but promises a mathematically survivable system, engineered to exploit momentary market inefficiency, while protecting capital from every known software and financial vulnerability.

**SRAD Sign-Off:** As Chief Engineer, I confirm that the official reference document for Project AEGIS system engineering is now complete with all five layers, including strategic updates (Shadow Trading & Telegram C2).

This constitution covers:
- **Data Infrastructure (Layer 1):** Ultra-speed and zero-trust networking
- **Alpha Generation (Layer 2):** Order book physics and net mathematical expectancy
- **Execution System (Layer 3):** Smart routing, orphan order protection, and server-side stop loss
- **Risk Management (Layer 4):** Hierarchical circuit breakers and absolute capital adaptation
- **Simulation & Control (Layer 5):** Pessimistic shadow trading and secure Arabic C2 interface

The document is now ready for handover to the development team (or for actual programming phase to begin by yourselves).

---

## 🚀 Getting Started

### Prerequisites
- Rust 1.65+
- Cargo
- Docker
- AWS CLI (for deployment)
- Apache Parquet tools

### Building
```bash
cd src/simulation_engine
cargo build --release
```

### Running Simulation
```bash
cd src/simulation_engine
cargo run --release
```

### CI/CD Setup
1. Configure GitHub Actions workflows
2. Set up AWS credentials
3. Configure Docker registry
4. Run deployment pipeline

---

## 📊 Performance Targets

- **Simulation Accuracy:** 99.9% parity with live trading
- **Backtest Speed:** 100x realtime performance
- **Deployment Reliability:** 99.95% successful deployments
- **Recovery Time:** < 1 second auto-recovery
- **Security:** Zero secret leakage incidents

---

*This system is for educational purposes only. Trading cryptocurrencies involves substantial risk.*