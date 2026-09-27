# Project AEGIS - Layer 1
## High-Frequency Trading System for Binance Perpetual Futures

### 🏛️ Official Reference Document: Layer 1 (Project AEGIS - Layer 1)
**Classification:** Secret / High-Frequency Systems Engineering (HFT)
**Target Environment:** Binance Perpetual Futures (Rust / Tokio Async Runtime)

## 🌐 Language Versions
- [English](README.md)
- [العربية](README.ar.md)

---

## 🔧 Architectural Philosophy

This layer adheres to three fundamental principles:

### 1. Time Determinism
Processing time for any data packet must be nearly constant (microsecond-level variance). No operations causing sudden delays (jitter) are permitted.

### 2. Zero-Trust Network
We assume connections will drop, packets will be lost, and Binance servers will experience congestion. The system is designed to detect and self-correct without human intervention.

### 3. Zero Dynamic Allocation
In the critical processing loop (Hot Path), requesting new memory (heap allocation) is strictly forbidden. All structures must be pre-allocated at startup.

---

## 📦 Technology Stack & Crates

Selected libraries based on performance benchmarks and avoiding "garbage collection":

- **Async Runtime:** Tokio (with multi_thread and worker_threads equal to physical CPU cores)
- **WebSocket Connections:** tokio-tungstenite with rustls for ultra-fast TLS encryption
- **Data Parsing:** simd-json (uses CPU SIMD instructions to parse JSON 3x faster than traditional serde_json)
- **Concurrent Data Structures:** crossbeam (lock-free channels) and rtrb (Real-Time Ring Buffer for thread data transfer)
- **Memory Management:** object-pool for object recycling to avoid delete/create operations

---

## 🧩 Core Components Architecture

### 3.1. Network & Transport Engine
Responsible for maintaining a live and continuous connection with Binance streams.

**Required Subscriptions (Per Symbol):**
- `<symbol>@depth@0ms` (instant differential order book updates)
- `<symbol>@aggTrade` (executed trades flow)
- `<symbol>@markPrice@1s` (mark price for liquidation avoidance and funding monitoring)
- `<symbol>@forceOrder` (forced liquidation orders - whale movement indicator)

**Engineering Design:** Dedicated OS Thread for each WebSocket connection to ensure data processing doesn't interfere with I/O network operations.

### 3.2. Ultra-Fast Deserialization Engine
**Function:** Convert textual JSON streams into Rust binary structs at minimal cost.

**Zero-Copy Parsing:** Instead of creating new Strings for each price or quantity, simd-json reads data directly from the network buffer as &str (references). This reduces memory consumption and CPU usage by up to 70%.

### 3.3. Local Order Book (LOB) Manager
This is the beating heart. The LOB must be exactly synchronized with Binance's real-time order book.

**Data Structure:**
- No HashMap (unordered)
- No traditional BTreeMap (slow in intensive updates)
- **Engineering Solution:** Dual-Array with binary indexing (Binary Indexed Tree) or custom Skip-List. We maintain two fixed-size pre-allocated arrays: one for buy orders (Bids) sorted descending, and another for sell orders (Asks) sorted ascending. Quantities are updated via direct indexing (O(1) complexity).

**Strict Sync Protocol:**
1. **Initial Snapshot:** Fetch order book state via REST API (`GET /fapi/v1/depth?limit=1000`)
2. **Buffering:** While fetching snapshot, store WebSocket updates in temporary Ring Buffer
3. **Merge & Validate:** Apply stored updates to snapshot. The lastUpdateId in snapshot must equal or be less than u (Update ID) in first WebSocket message
4. **Continuous Checksum:** Every WebSocket message contains U (First Update ID), u (Last Update ID), and pu (Previous Update ID). Mandatory rule: pu for current message must equal u for previous message

### 3.4. Internal Event Bus
**Function:** Transfer data from LOB to "signal generation engine" (Layer 2).

**Engineering Design:** Using rtrb (Ring Buffer) single-producer/single-consumer (SPSC). This prevents any Mutex usage that causes Thread Contention.

---

## ⚠️ Failure & Success Analysis

Applying "double lens" to every critical point in Layer 1:

### Failure Scenario 1: Silent WebSocket Disconnection
**Problem:** Connection may drop without server sending Close signal. Bot continues trading based on "dead" order book (Stale Data), leading to incorrect trades and catastrophic losses.

**Radical Hardening (Circuit Breaker):**
- Apply heartbeat watchdog. If no pong or market data received within 2000ms, consider connection dead
- Immediately fire MarketDataHalt event
- Bot cancels all open orders via REST API as precautionary measure, then begins exponential backoff reconnection protocol

### Failure Scenario 2: Packet Loss / Sequence Gap
**Problem:** Network congestion leads to lost WebSocket messages. If pu ≠ previous u, local order book becomes corrupted. Trading now is blind gambling.

**Radical Hardening (State Recovery):**
- Upon detecting sequence gap, engine immediately stops sending trading signals
- Entire local LOB is purged
- New Snapshot request sent via REST API
- During Snapshot wait, new execution orders are ignored (Trading Suspended) to protect capital

### Failure Scenario 3: Data Flood & Memory Exhaustion
**Problem:** During violent market swings (Flash Crashes), Binance sends thousands of updates/sec. If "strategy engine" slower than "data engine", communication channels fill, leading to OOM crash or trading with seconds-delayed data (Latency Spike).

**Radical Hardening (Backpressure & Drop Strategy):**
- Use bounded channels with limited capacity (e.g., 10,000 messages)
- "Newest is most important" strategy (Drop-Oldest): If channel full, data engine deletes oldest unprocessed message, replacing with new message. In Hyper-Scalping, 50ms old data is ancient history with no value. Better to make decision on latest price vs. waiting to process dead prices

### Failure Scenario 4: REST API Rate Limit Exceeded
**Problem:** During repeated disconnections, bot may attempt to fetch Snapshot hundreds of times, resulting in IP ban from Binance (HTTP 429 / 418 Ban).

**Radical Hardening (Rate Limit Shield):**
- Apply local token bucket limiter preventing more than one Snapshot request every 3 seconds per trading symbol
- Use multiple API keys (Multi-API Key Routing) dedicated to public data only to distribute load and avoid banning primary key responsible for execution

---

## 💡 CPU & Memory Optimization Strategy

To achieve near-zero latency:

### Thread Pinning
Using core_affinity library to pin LOB Thread to specific CPU core (CPU Core 1), preventing OS from moving it (Context Switching)

### Avoid Heap Allocation
All message structs created on stack

### Data Locality
Design LOB data structure so prices and quantities are contiguous in memory (Array of Structs vs. Struct of Arrays) to maximize processor cache utilization (L1/L2 Cache)

---

## 🚀 Getting Started

### Prerequisites
- Rust 1.65+
- Cargo
- Binance API credentials (for production use)

### Building
```bash
cd src/binance_hft
cargo build --release
```

### Running
```bash
cd src/binance_hft
cargo run --release
```

---

## 📊 Performance Targets

- **Latency:** < 50 microseconds for order book update processing
- **Throughput:** > 100,000 updates per second
- **Memory Usage:** < 100MB baseline
- **Recovery Time:** < 2 seconds after disconnection

---

## 🔒 Security Considerations

- All API keys stored securely (environment variables)
- Rate limiting to prevent bans
- Secure WebSocket connections with TLS
- Input validation for all external data

---

*This system is for educational purposes only. Trading cryptocurrencies involves substantial risk.*