//! Tick-Level Data Lake implementation
//!
//! The data lake's only contract is: *hand the backtester an ordered sequence of real
//! market events for the requested window*. Nothing here is invented - when the lake
//! is empty it says so and the run fails, because a backtest fed with made-up ticks
//! still produces a report, and reports in this project are read as evidence.
//!
//! # Accepted files
//!
//! Every `*.parquet`, `*.jsonl` or `*.json` file directly under `data_path` is read.
//! Two row shapes are accepted, in this order of preference:
//!
//! 1. A serialized [`MarketEvent`] (`{"Trade": {...}}`, `{"OrderBookSnapshot": {...}}`,
//!    `{"FundingRate": {...}}`) - the shape this crate writes, so a dumped run can be
//!    replayed verbatim.
//! 2. A flat row (see [`EventRow`]) with the columns an exporter produces.
//!
//! Events are filtered to the configured window and symbol list, then sorted by
//! timestamp with snapshots placed before the trades that share their millisecond: the
//! queue a limit order sits in is defined by the book *as of* the print, so applying
//! the print first would let a fill be judged against a stale book.

use crate::core::{
    FundingRateEvent, MarketEvent, OrderBookSnapshot, PriceLevel, SimulationConfig,
    SimulationError, TradeEvent, TradeSide,
};
use crate::core::constants::MIN_EPOCH_MILLIS;
use chrono::DateTime;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Row, RowAccessor};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Depth the flat column convention is read to. A five-level OBI needs five; twenty
/// covers a spread/depth check without needing a second code path.
const MAX_RECORDED_LEVELS: usize = 20;

/// Data Lake for tick-level market data
#[derive(Clone)]
pub struct DataLake {
    data_path: String,
    is_initialized: bool,
}

impl DataLake {
    /// Create a new Data Lake
    pub fn new() -> Self {
        Self {
            data_path: String::new(),
            is_initialized: false,
        }
    }

    /// Initialize the data lake with data path
    ///
    /// The path is checked here rather than at first read: an operator pointing the
    /// simulator at a directory that does not exist needs to hear it now, not after a
    /// run that reported "no signals".
    pub fn initialize(&mut self, data_path: &str) -> Result<(), SimulationError> {
        if data_path.trim().is_empty() {
            return Err(SimulationError::InvalidConfig(
                "Data path must not be empty".to_string(),
            ));
        }

        let path = Path::new(data_path);
        if !path.exists() {
            return Err(SimulationError::DataLoadingError(format!(
                "Data path does not exist: {} - record tick data there, or point data_path at the lake",
                data_path
            )));
        }
        if !path.is_dir() {
            return Err(SimulationError::DataLoadingError(format!(
                "Data path is not a directory: {}",
                data_path
            )));
        }

        self.data_path = data_path.to_string();
        self.is_initialized = true;

        match self.discover_files() {
            Ok(files) => info!(
                "Data Lake initialized with path: {} ({} event file(s) found)",
                data_path,
                files.len()
            ),
            Err(e) => warn!("Data Lake initialized at {} but could not be listed: {}", data_path, e),
        }

        Ok(())
    }

    /// The files the lake will read, in a deterministic order so that two runs over
    /// the same directory see the same sequence.
    pub fn discover_files(&self) -> Result<Vec<PathBuf>, SimulationError> {
        if !self.is_initialized {
            return Err(SimulationError::DataLoadingError(
                "Data Lake not initialized".to_string(),
            ));
        }

        let entries = fs::read_dir(&self.data_path).map_err(|e| {
            SimulationError::DataLoadingError(format!("Cannot list {}: {}", self.data_path, e))
        })?;

        let mut files: Vec<PathBuf> = entries
            .filter_map(|entry| match entry {
                Ok(entry) => Some(entry.path()),
                Err(e) => {
                    warn!("Skipping unreadable directory entry in {}: {}", self.data_path, e);
                    None
                }
            })
            .filter(|path| is_event_file(path))
            .collect();

        files.sort();
        Ok(files)
    }

    /// Load the events for one backtest run: filtered to its window and symbols, and
    /// ordered for replay.
    pub fn collect_events(&self, config: &SimulationConfig) -> Result<Vec<MarketEvent>, SimulationError> {
        let files = self.discover_files()?;

        if files.is_empty() {
            return Err(SimulationError::DataLoadingError(format!(
                "No event files (*.parquet, *.jsonl, *.json) under {} - a backtest needs recorded data",
                self.data_path
            )));
        }

        let start_ms = config.start_time.timestamp_millis();
        let end_ms = config.end_time.timestamp_millis();

        let mut events = Vec::new();
        let mut loaded = 0usize;
        let mut outside_window = 0usize;
        let mut other_symbols = 0usize;

        for file in &files {
            let file_events = self.read_file(file)?;
            loaded += file_events.len();
            events.extend(file_events);
        }

        let mut kept = Vec::with_capacity(events.len());
        for event in events {
            let timestamp = event_timestamp(&event);
            if timestamp < start_ms || timestamp > end_ms {
                outside_window += 1;
                continue;
            }
            if !config.symbols.is_empty()
                && !config
                    .symbols
                    .iter()
                    .any(|symbol| symbol == event_symbol(&event))
            {
                other_symbols += 1;
                continue;
            }
            kept.push(event);
        }

        // A replay must be reproducible: same files in, same order out, every time.
        kept.sort_by(|a, b| {
            event_timestamp(a)
                .cmp(&event_timestamp(b))
                .then_with(|| event_kind_rank(a).cmp(&event_kind_rank(b)))
        });

        info!(
            "Data Lake served {} event(s) for [{}, {}] on {} symbol(s) from {} file(s) \
             ({} read, {} outside the window, {} other symbols)",
            kept.len(),
            start_ms,
            end_ms,
            config.symbols.len(),
            files.len(),
            loaded,
            outside_window,
            other_symbols
        );
        if kept.is_empty() {
            warn!(
                "The window [{}, {}] has no events in {}; the run will be failed rather than reported",
                start_ms, end_ms, self.data_path
            );
        }

        Ok(kept)
    }

    /// Every event the lake holds, unfiltered. Used by an external feed pushing into a
    /// run that is already active; a replay uses [`DataLake::collect_events`].
    pub fn load_all_events(&self) -> Result<Vec<MarketEvent>, SimulationError> {
        let mut events = Vec::new();
        for file in self.discover_files()? {
            events.extend(self.read_file(&file)?);
        }
        events.sort_by(|a, b| {
            event_timestamp(a)
                .cmp(&event_timestamp(b))
                .then_with(|| event_kind_rank(a).cmp(&event_kind_rank(b)))
        });
        Ok(events)
    }

    /// Feed data to backtesting engine
    pub async fn feed_data(
        &self,
        backtesting_engine: &crate::components::BacktestingEngine,
    ) -> Result<usize, SimulationError> {
        if !self.is_initialized {
            return Ok(0); // Nothing to do if not initialized
        }

        let events = self.load_all_events()?;
        let mut delivered = 0usize;

        for event in events {
            // The engine owns the replay rules (window bounds, ordering), so a feed
            // that pushes something it rejects is reported and skipped rather than
            // taking the whole ingest down.
            if let Err(e) = backtesting_engine.process_market_event(event).await {
                debug!("Market event rejected by the engine: {}", e);
                continue;
            }
            delivered += 1;
        }

        Ok(delivered)
    }

    fn read_file(&self, path: &Path) -> Result<Vec<MarketEvent>, SimulationError> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("parquet") => self.read_parquet(path),
            Some("json") | Some("jsonl") => self.read_json(path),
            // A lake full of CSV is a real finding, so it is logged rather than being
            // silently read as zero rows.
            other => {
                warn!(
                    "Ignoring {}: unsupported extension {:?} (expected .parquet, .jsonl or .json)",
                    path.display(),
                    other
                );
                Ok(Vec::new())
            }
        }
    }

    fn read_json(&self, path: &Path) -> Result<Vec<MarketEvent>, SimulationError> {
        let text = fs::read_to_string(path).map_err(|e| {
            SimulationError::DataLoadingError(format!("Cannot read {}: {}", path.display(), e))
        })?;

        // A leading bracket means a whole-file JSON array rather than JSON Lines.
        let values = if text.trim_start().starts_with("[") {
            serde_json::from_str::<Vec<Value>>(&text).map_err(|e| {
                SimulationError::DataLoadingError(format!("{}: not a valid JSON array: {}", path.display(), e))
            })?
        } else {
            // JSON Lines: one object per line, which is how a `MarketEvent` dump is
            // written and how tick files are usually streamed to disk.
            let mut values = Vec::new();
            for (index, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                values.push(
                    serde_json::from_str::<Value>(line).map_err(|e| {
                        SimulationError::DataLoadingError(format!(
                            "{}:{}: {}",
                            path.display(),
                            index + 1,
                            e
                        ))
                    })?,
                );
            }
            values
        };

        self.materialize(path, values)
    }

    fn read_parquet(&self, path: &Path) -> Result<Vec<MarketEvent>, SimulationError> {
        let file = fs::File::open(path).map_err(|e| {
            SimulationError::DataLoadingError(format!("Cannot open {}: {}", path.display(), e))
        })?;

        let reader = SerializedFileReader::new(file)?;
        // The record (row) API rather than the arrow columnar path: this crate depends
        // on `parquet` but deliberately not on the `arrow` umbrella (see Cargo.toml).
        let rows = reader.get_row_iter(None)?;

        let mut values = Vec::new();
        for row in rows {
            let row = row?;
            values.push(row_to_json(&row));
        }

        self.materialize(path, values)
    }

    /// Turn raw rows into events, reporting rows that are not market data instead of
    /// dropping them quietly: a file of the wrong shape has to be visible in the log of
    /// the run that ignored it.
    fn materialize(&self, path: &Path, values: Vec<Value>) -> Result<Vec<MarketEvent>, SimulationError> {
        let total = values.len();
        let mut events = Vec::with_capacity(total);
        let mut unreadable = 0usize;
        let mut first_error: Option<String> = None;

        for value in values {
            match parse_event_value(&value) {
                Ok(event) => events.push(event),
                Err(e) => {
                    unreadable += 1;
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        if unreadable > 0 {
            warn!(
                "{}: {} of {} row(s) are not market events (first: {})",
                path.display(),
                unreadable,
                total,
                first_error.unwrap_or_default()
            );
        }
        if events.is_empty() && total > 0 {
            return Err(SimulationError::DataLoadingError(format!(
                "{}: {} row(s) read, none of them a market event - check the column names \
                 against EventRow (order books in Parquet must use the paired \
                 bid_price_i / bid_qty_i columns, because the row API cannot read nested \
                 list columns)",
                path.display(),
                total
            )));
        }

        Ok(events)
    }

    /// Check if data lake is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Where the lake reads from (empty until initialized).
    pub fn data_path(&self) -> &str {
        &self.data_path
    }
}

/// The first typed read that succeeds. Tick exporters disagree about whether a price
/// is a double or a string and whether a timestamp is an int64 or a timestamp, so a
/// column is read across the physical types instead of failing the whole file.
fn column_value(row: &Row, index: usize) -> Option<Value> {
    if let Ok(value) = row.get_double(index) {
        return Some(Value::from(value));
    }
    if let Ok(value) = row.get_float(index) {
        return Some(Value::from(f64::from(value)));
    }
    if let Ok(value) = row.get_long(index) {
        return Some(Value::from(value));
    }
    if let Ok(value) = row.get_int(index) {
        return Some(Value::from(i64::from(value)));
    }
    if let Ok(text) = row.get_string(index) {
        return Some(Value::String(text.clone()));
    }
    if let Ok(flag) = row.get_bool(index) {
        return Some(Value::Bool(flag));
    }
    // A null (or a list/group the record API cannot flatten) is left out, which reads
    // as an absent field to EventRow rather than as a fabricated zero.
    None
}

/// Project one Parquet row onto a JSON object keyed by column name.
///
/// `Row` carries no `to_json_value` in the pinned `parquet` release, and the arrow path
/// would mean re-adding the umbrella crate this crate dropped (arrow <= 54 does not
/// build against chrono >= 0.4.40), so rows are flattened through the record API's
/// typed getters instead.
fn row_to_json(row: &Row) -> Value {
    let mut object = serde_json::Map::new();

    // `get_column_iter` yields columns in file-schema order, so the same index serves
    // for the name and for the positional getter above.
    for (index, (name, _)) in row.get_column_iter().enumerate() {
        if let Some(value) = column_value(row, index) {
            object.insert(name.to_string(), value);
        }
    }

    Value::Object(object)
}

fn is_event_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("parquet") | Some("jsonl") | Some("json")
    )
}

fn event_timestamp(event: &MarketEvent) -> i64 {
    match event {
        MarketEvent::OrderBookSnapshot(snapshot) => snapshot.timestamp,
        MarketEvent::Trade(trade) => trade.timestamp,
        MarketEvent::FundingRate(funding) => funding.timestamp,
    }
}

fn event_symbol(event: &MarketEvent) -> &str {
    match event {
        MarketEvent::OrderBookSnapshot(snapshot) => &snapshot.symbol,
        MarketEvent::Trade(trade) => &trade.symbol,
        MarketEvent::FundingRate(funding) => &funding.symbol,
    }
}

/// Same-millisecond ordering: the book defines the queue, the print fills it, funding
/// settles what is left open.
fn event_kind_rank(event: &MarketEvent) -> u8 {
    match event {
        MarketEvent::OrderBookSnapshot(_) => 0,
        MarketEvent::Trade(_) => 1,
        MarketEvent::FundingRate(_) => 2,
    }
}

/// One recorded row, in the flat shape tick exporters write:
///
/// | row | fields |
/// |-----|--------|
/// | order book snapshot | `symbol`, `timestamp`, and either `bids`/`asks` (array of `{price, quantity}`, or a JSON string holding one) or the paired columns `bid_price_0`, `bid_qty_0`, `ask_price_0`, `ask_qty_0`, ... |
/// | trade | `symbol`, `timestamp`, `price`, `quantity`, and `side` (`"buy"`/`"sell"`) or `is_buyer_maker` |
/// | funding | `symbol`, `timestamp`, `funding_rate` (or `rate`) |
///
/// `timestamp` is epoch milliseconds or an RFC 3339 string; numbers may be JSON
/// numbers or quoted strings. Exported tick data does all of these, and a backtest that
/// silently loses the rows it could not parse is worse than one that reads the sloppy
/// version.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct EventRow {
    /// Optional explicit tag: `"snapshot"`, `"trade"` or `"funding"`.
    kind: Option<String>,
    symbol: Option<String>,
    timestamp: Option<Value>,
    price: Option<Value>,
    quantity: Option<Value>,
    side: Option<String>,
    is_buyer_maker: Option<Value>,
    funding_rate: Option<Value>,
    bids: Option<Value>,
    asks: Option<Value>,
    /// Everything else, so the paired `bid_price_i` columns can be found without
    /// inventing 80 struct fields.
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

impl EventRow {
    fn into_event(self) -> Result<MarketEvent, String> {
        let symbol = match self.symbol.as_deref().map(str::trim) {
            Some(symbol) if !symbol.is_empty() => symbol.to_string(),
            _ => return Err("row has no symbol".to_string()),
        };
        let timestamp = timestamp_millis(self.timestamp.as_ref(), &symbol)?;
        let kind = self
            .kind
            .as_deref()
            .map(|kind| kind.trim().to_ascii_lowercase())
            .unwrap_or_default();

        // Two column names carry the same number: `funding_rate` in the dumps this crate
        // writes, `rate` in the venue's own funding stream. Detection has to accept both,
        // or an untagged `rate` row walks into the trade branch and is lost to a
        // "no price" error - a funding charge silently missing from the replay.
        let rate_column = self
            .funding_rate
            .as_ref()
            .map(|value| ("funding_rate", value))
            .or_else(|| self.extra.get("rate").map(|value| ("rate", value)));

        if kind == "funding" || (kind.is_empty() && rate_column.is_some()) {
            let (column, value) = rate_column.ok_or_else(|| {
                format!("{}: a row tagged as funding carries no rate column", symbol)
            })?;
            let rate = number(Some(value), &symbol, column)?;
            return Ok(MarketEvent::FundingRate(FundingRateEvent {
                symbol,
                timestamp,
                rate,
            }));
        }

        let bids = parse_levels(self.bids.as_ref(), "bid", &self.extra)?;
        let asks = parse_levels(self.asks.as_ref(), "ask", &self.extra)?;
        if kind == "snapshot" || kind == "book" || (kind.is_empty() && (!bids.is_empty() || !asks.is_empty())) {
            if bids.is_empty() || asks.is_empty() {
                return Err(format!(
                    "{}: a snapshot row needs both bids and asks",
                    symbol
                ));
            }
            return Ok(MarketEvent::OrderBookSnapshot(OrderBookSnapshot {
                symbol,
                timestamp,
                bids,
                asks,
            }));
        }

        let price = number(self.price.as_ref(), &symbol, "price")?;
        let quantity = number(self.quantity.as_ref(), &symbol, "quantity")?;
        Ok(MarketEvent::Trade(TradeEvent {
            symbol,
            timestamp,
            price,
            quantity,
            side: trade_side(self.side.as_deref(), self.is_buyer_maker.as_ref()),
        }))
    }
}

/// Parse one raw row. `MarketEvent`'s own serialized shape is tried first so a dump
/// written by this crate replays without a mapping step.
fn parse_event_value(value: &Value) -> Result<MarketEvent, String> {
    if let Ok(event) = serde_json::from_value::<MarketEvent>(value.clone()) {
        return Ok(event);
    }

    let row: EventRow = serde_json::from_value(value.clone())
        .map_err(|e| format!("not a market event row: {}", e))?;
    row.into_event()
}

fn timestamp_millis(value: Option<&Value>, symbol: &str) -> Result<i64, String> {
    let value = value.ok_or_else(|| format!("{}: row has no timestamp", symbol))?;

    let millis = match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64))
            .ok_or_else(|| format!("{}: timestamp {} is out of range", symbol, number))?,
        Value::String(text) => {
            let text = text.trim();
            match text.parse::<f64>() {
                Ok(parsed) => parsed as i64,
                Err(_) => DateTime::parse_from_rfc3339(text)
                    .map(|moment| moment.timestamp_millis())
                    .map_err(|_| format!("{}: unreadable timestamp {:?}", symbol, text))?,
            }
        }
        Value::Bool(_) | Value::Null | Value::Array(_) | Value::Object(_) => {
            return Err(format!(
                "{}: timestamp must be epoch milliseconds or an RFC 3339 string, got {}",
                symbol, value
            ))
        }
    };

    if millis < MIN_EPOCH_MILLIS {
        // Seconds-epoch data would otherwise land outside every window and the run
        // would report "no signals" instead of "wrong units".
        return Err(format!(
            "{}: timestamp {} is not a millisecond epoch (before 2001); multiply a seconds epoch by 1000",
            symbol, millis
        ));
    }

    Ok(millis)
}

fn number(value: Option<&Value>, symbol: &str, field: &str) -> Result<f64, String> {
    let value = value.ok_or_else(|| format!("{}: row has no {}", symbol, field))?;
    scalar_f64(value).ok_or_else(|| format!("{}: {} is not numeric ({})", symbol, field, value))
}

fn scalar_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().or_else(|| number.as_i64().map(|value| value as f64)),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn scalar_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::Number(number) => number.as_i64().map(|value| value != 0),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "t" | "y" | "yes" => Some(true),
            "false" | "0" | "f" | "n" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Both encodings a tick file carries a book in: a nested array (or a JSON string
/// holding one) and the paired flat columns that parquet writers use, since the row API
/// reads nested lists only with caveats.
fn parse_levels(
    value: Option<&Value>,
    side: &str,
    extra: &HashMap<String, Value>,
) -> Result<Vec<PriceLevel>, String> {
    let mut levels = Vec::new();

    if let Some(value) = value {
        let array = match value {
            Value::String(text) => serde_json::from_str::<Value>(text)
                .map_err(|e| format!("{}: nested levels are not JSON ({})", side, e))?,
            other => other.clone(),
        };

        let items = match array {
            Value::Array(items) => items,
            Value::Null => Vec::new(),
            other => return Err(format!("{}: expected an array of levels, got {}", side, other)),
        };

        for item in items.into_iter().take(MAX_RECORDED_LEVELS) {
            match level_from_value(&item) {
                Some(level) => levels.push(level),
                None => return Err(format!("{}: level {} has no readable price/quantity", side, item)),
            }
        }
        return Ok(levels);
    }

    for index in 0..MAX_RECORDED_LEVELS {
        let price = extra.get(&format!("{}_price_{}", side, index));
        let quantity = extra
            .get(&format!("{}_qty_{}", side, index))
            .or_else(|| extra.get(&format!("{}_quantity_{}", side, index)));

        match (price, quantity) {
            (Some(price), Some(quantity)) => {
                let price = scalar_f64(price).ok_or_else(|| format!("{}: price {} is not numeric", side, price))?;
                let quantity =
                    scalar_f64(quantity).ok_or_else(|| format!("{}: quantity {} is not numeric", side, quantity))?;
                levels.push(PriceLevel { price, quantity });
            }
            // A half-present level is a broken file, not the end of the book.
            (Some(_), None) | (None, Some(_)) => {
                return Err(format!(
                    "{}_level {}: only one of price/quantity is present",
                    side, index
                ))
            }
            (None, None) => break,
        }
    }

    Ok(levels)
}

fn level_from_value(item: &Value) -> Option<PriceLevel> {
    match item {
        Value::Array(pair) if pair.len() >= 2 => {
            let price = scalar_f64(&pair[0])?;
            let quantity = scalar_f64(&pair[1])?;
            Some(PriceLevel { price, quantity })
        }
        Value::Object(map) => {
            let price = ["price", "p"]
                .iter()
                .find_map(|key| map.get(*key))
                .and_then(scalar_f64)?;
            let quantity = ["quantity", "qty", "q", "size", "amount"]
                .iter()
                .find_map(|key| map.get(*key))
                .and_then(scalar_f64)?;
            Some(PriceLevel { price, quantity })
        }
        _ => None,
    }
}

/// The aggressor's side. `is_buyer_maker` is read as Binance writes it: true means the
/// *buyer* was resting, so the aggressor - the side that pays the taker fee - sold.
fn trade_side(side: Option<&str>, is_buyer_maker: Option<&Value>) -> TradeSide {
    if let Some(text) = side {
        match text.trim().to_ascii_lowercase().as_str() {
            "buy" | "b" | "1" | "true" => return TradeSide::Buy,
            "sell" | "s" | "-1" | "0" | "false" => return TradeSide::Sell,
            _ => {}
        }
    }

    if let Some(value) = is_buyer_maker {
        if let Some(flag) = scalar_bool(value) {
            return if flag { TradeSide::Sell } else { TradeSide::Buy };
        }
    }

    // The simulated path prices fills off the print, not off the aggressor flag, so a
    // missing flag is not worth failing a replay over.
    TradeSide::Buy
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn row(text: &str) -> MarketEvent {
        let value: Value = serde_json::from_str(text).expect("test fixture is valid JSON");
        parse_event_value(&value).expect("fixture maps to an event")
    }

    /// Year 2100, far enough to be "no upper bound" inside chrono's range.
    const FAR_FUTURE: i64 = 4_102_444_800_000;

    fn utc(millis: i64) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(millis)
            .single()
            .expect("millisecond epoch inside chrono's range")
    }

    fn config(start_ms: i64, end_ms: i64, symbols: &[&str]) -> SimulationConfig {
        SimulationConfig {
            data_path: String::new(),
            start_time: utc(start_ms),
            end_time: utc(end_ms),
            playback_speed: 1.0,
            initial_capital: 10_000.0,
            leverage: 10.0,
            symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
            ..SimulationConfig::default()
        }
    }

    #[test]
    fn a_flat_trade_row_maps_to_a_trade_event() {
        let event = row(
            r#"{"symbol":"BTCUSDT","timestamp":1700000000000,"price":"40000.5","quantity":0.5,"side":"sell"}"#,
        );

        match event {
            MarketEvent::Trade(trade) => {
                assert_eq!(trade.price, 40_000.5, "a quoted number still parses");
                assert_eq!(trade.quantity, 0.5);
                assert_eq!(trade.side, TradeSide::Sell);
            }
            other => panic!("expected a trade, got {:?}", other),
        }
    }

    #[test]
    fn buyer_maker_is_inverted_into_the_aggressor_side() {
        let aggressive_sell = row(
            r#"{"symbol":"BTCUSDT","timestamp":1700000000000,"price":40000.0,"quantity":1.0,"is_buyer_maker":true}"#,
        );
        assert!(matches!(
            aggressive_sell,
            MarketEvent::Trade(TradeEvent { side: TradeSide::Sell, .. })
        ));

        let aggressive_buy = row(
            r#"{"symbol":"BTCUSDT","timestamp":1700000000000,"price":40000.0,"quantity":1.0,"is_buyer_maker":false}"#,
        );
        assert!(matches!(
            aggressive_buy,
            MarketEvent::Trade(TradeEvent { side: TradeSide::Buy, .. })
        ));
    }

    #[test]
    fn books_are_read_from_nested_arrays_and_from_paired_columns() {
        let nested = row(
            r#"{"symbol":"ETHUSDT","timestamp":1700000000000,"bids":[["3000.0","2.0"],["2999.0","1.0"]],"asks":[{"price":3001.0,"quantity":0.5}]}"#,
        );
        match nested {
            MarketEvent::OrderBookSnapshot(book) => {
                assert_eq!(book.bids.len(), 2);
                assert_eq!(book.bids[0].price, 3000.0);
                assert_eq!(book.bids[0].quantity, 2.0);
                assert_eq!(book.asks.len(), 1);
            }
            other => panic!("expected a snapshot, got {:?}", other),
        }

        let flat = row(
            r#"{"symbol":"ETHUSDT","timestamp":1700000000000,"bid_price_0":3000.0,"bid_qty_0":2.0,"bid_price_1":2999.0,"bid_qty_1":1.0,"ask_price_0":3001.0,"ask_qty_0":0.25}"#,
        );
        match flat {
            MarketEvent::OrderBookSnapshot(book) => {
                assert_eq!(book.bids.len(), 2);
                assert_eq!(book.asks.len(), 1);
                assert_eq!(book.asks[0].quantity, 0.25);
            }
            other => panic!("expected a snapshot, got {:?}", other),
        }
    }

    #[test]
    fn a_book_written_as_a_json_string_column_is_still_a_book() {
        let event = row(
            r#"{"symbol":"BTCUSDT","timestamp":1700000000000,"bids":"[[40000.0,1.0]]","asks":"[[40001.0,2.0]]"}"#,
        );
        assert!(matches!(event, MarketEvent::OrderBookSnapshot(_)));
    }

    #[test]
    fn funding_rows_are_read_under_both_column_names() {
        for field in ["funding_rate", "rate"] {
            let event = row(&format!(
                r#"{{"symbol":"BTCUSDT","timestamp":1700000000000,"{}":0.0001}}"#,
                field
            ));
            match event {
                MarketEvent::FundingRate(funding) => assert_eq!(funding.rate, 0.0001),
                other => panic!("{}: expected funding, got {:?}", field, other),
            }
        }
    }

    #[test]
    fn the_crates_own_event_shape_round_trips() {
        let original = MarketEvent::Trade(TradeEvent {
            symbol: "BTCUSDT".to_string(),
            timestamp: 1700000000000,
            price: 40000.5,
            quantity: 0.5,
            side: TradeSide::Buy,
        });

        let text = serde_json::to_string(&original).expect("events serialize");
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(format!("{:?}", parse_event_value(&value).unwrap()), format!("{:?}", original));
    }

    #[test]
    fn rfc3339_timestamps_are_accepted() {
        let event = row(
            r#"{"symbol":"BTCUSDT","timestamp":"2023-11-14T22:13:20Z","price":40000.0,"quantity":1.0,"side":"buy"}"#,
        );
        assert_eq!(event_timestamp(&event), 1_700_000_000_000);
    }

    #[test]
    fn seconds_epoch_data_is_refused_not_silently_dropped() {
        let value: Value =
            serde_json::from_str(r#"{"symbol":"BTCUSDT","timestamp":1700000000,"price":1.0,"quantity":1.0}"#)
                .unwrap();

        let error = parse_event_value(&value).expect_err("seconds look like a pre-2001 millisecond epoch");
        assert!(error.contains("millisecond epoch"), "{}", error);
    }

    #[test]
    fn a_snapshot_without_asks_is_an_error_not_a_half_book() {
        let value: Value = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","timestamp":1700000000000,"bids":[[40000.0,1.0]],"kind":"snapshot"}"#,
        )
        .unwrap();

        assert!(parse_event_value(&value).unwrap_err().contains("bids and asks"));
    }

    #[test]
    fn an_uninitialized_lake_reads_nothing_and_says_so() {
        let lake = DataLake::new();
        let error = lake
            .discover_files()
            .expect_err("no path, no files");
        assert!(matches!(error, SimulationError::DataLoadingError(_)));
    }

    #[test]
    fn a_missing_directory_is_reported_at_open() {
        let mut lake = DataLake::new();
        let error = lake
            .initialize("/definitely/not/a/lake")
            .expect_err("the path has to exist");
        assert!(error.to_string().contains("does not exist"), "{}", error);
    }

    #[test]
    fn an_empty_directory_is_reported_at_read() {
        let directory = std::env::temp_dir().join(format!("aegis_lake_empty_{}", std::process::id()));
        fs::create_dir_all(&directory).expect("temp dir");

        let mut lake = DataLake::new();
        lake.initialize(directory.to_str().expect("utf-8 temp path"))
            .expect("the directory exists");

        let error = lake
            .collect_events(&config(MIN_EPOCH_MILLIS, FAR_FUTURE, &[]))
            .expect_err("a lake with no files cannot back a run");
        assert!(error.to_string().contains("No event files"), "{}", error);

        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_jsonl_file_is_sliced_ordered_and_limited_to_the_window() {
        let directory = std::env::temp_dir().join(format!("aegis_lake_jsonl_{}", std::process::id()));
        fs::create_dir_all(&directory).expect("temp dir");

        let start_ms = 1_700_000_000_000;
        // Deliberately out of order on disk, and including a row for another symbol and
        // a row before the window: all three have to be handled by the loader.
        let lines = [
            format!(r#"{{"symbol":"BTCUSDT","timestamp":{},"price":40400.0,"quantity":1.0,"side":"buy"}}"#, start_ms + 2_000),
            format!(r#"{{"symbol":"BTCUSDT","timestamp":{},"bids":[[40000.0,8.0]],"asks":[[40001.0,0.5]]}}"#, start_ms + 1_000),
            format!(r#"{{"symbol":"BTCUSDT","timestamp":{},"price":40000.0,"quantity":1.0,"side":"sell"}}"#, start_ms + 1_000),
            r#"{"symbol":"ETHUSDT","timestamp":1700000001000,"price":3000.0,"quantity":1.0,"side":"buy"}"#.to_string(),
            format!(r#"{{"symbol":"BTCUSDT","timestamp":{},"price":39000.0,"quantity":1.0,"side":"buy"}}"#, start_ms - 60_000),
        ]
        .join("\n");
        fs::write(directory.join("events.jsonl"), lines).expect("fixture written");

        let mut lake = DataLake::new();
        lake.initialize(directory.to_str().expect("utf-8 temp path"))
            .expect("the directory exists");

        let events = lake
            .collect_events(&config(start_ms, start_ms + 5_000, &["BTCUSDT"]))
            .expect("events in the window");

        assert_eq!(events.len(), 3, "other symbols and out-of-window rows are dropped");
        assert_eq!(event_timestamp(&events[0]), start_ms + 1_000);
        assert!(
            matches!(events[0], MarketEvent::OrderBookSnapshot(_)),
            "the book is applied before the trade that shares its millisecond"
        );
        assert!(matches!(events[1], MarketEvent::Trade(_)));
        assert_eq!(event_timestamp(&events[2]), start_ms + 2_000);

        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_file_of_foreign_rows_fails_the_load_instead_of_reading_as_zero_events() {
        let directory = std::env::temp_dir().join(format!("aegis_lake_junk_{}", std::process::id()));
        fs::create_dir_all(&directory).expect("temp dir");
        fs::write(
            directory.join("orders.jsonl"),
            r#"{"id":1,"side":"buy","size":2}"#.to_string(),
        )
        .expect("fixture written");

        let mut lake = DataLake::new();
        lake.initialize(directory.to_str().expect("utf-8 temp path"))
            .expect("the directory exists");

        let error = lake
            .collect_events(&config(MIN_EPOCH_MILLIS, FAR_FUTURE, &[]))
            .expect_err("a file with no market events must not read as an empty lake");
        assert!(error.to_string().contains("none of them a market event"), "{}", error);

        fs::remove_dir_all(&directory).ok();
    }
}
