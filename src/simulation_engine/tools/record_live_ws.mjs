#!/usr/bin/env node
/*
 * AEGIS Layer 5 - data lake recorder.
 *
 * Why this exists instead of "download history from Binance Vision": the archives at
 * data.binance.vision carry no order book prices. `bookDepth` is a percentage-band
 * notional aggregate (timestamp, percentage, depth, notional - no price levels) and
 * `bookTicker` was L1 only, discontinued after 2024-03-30. The strategy in this project
 * enters on order-book imbalance and fills off the queue in front of the best bid, so a
 * replay without a book cannot produce a single entry: it would report "no events in the
 * window", or with trades only, zero fills and a 0% execution quality. Recording the live
 * partial-book stream is the way to get real L2 evidence for free.
 *
 * It writes the flat record shape the lake reads (see README section 5.1), one JSON object
 * per line:
 *   {"symbol":"BTCUSDT","timestamp":<ms>,"bids":[[p,q],...],"asks":[[p,q],...]}
 *   {"symbol":"BTCUSDT","timestamp":<ms>,"price":p,"quantity":q,"is_buyer_maker":bool}
 *   {"symbol":"BTCUSDT","timestamp":<ms>,"funding_rate":r}
 * Nothing else: a comment line or a metadata header would make the lake fail the file, which
 * is deliberate - the reader refuses to pretend it understood a row it did not.
 *
 * Usage (Node 22+ has WebSocket built in, so there is nothing to install):
 *   node src/simulation_engine/tools/record_live_ws.mjs --data ./data --minutes 30
 *   node src/simulation_engine/tools/record_live_ws.mjs --market spot --symbols BTCUSDT,ETHUSDT
 *   node src/simulation_engine/tools/record_live_ws.mjs --book bookTicker   # if depth20 is throttled
 *
 * Timestamps are checked to be milliseconds in 2001..2100 before they are written; a
 * microsecond stream would otherwise land outside every window the replay can be asked
 * about and look like empty data.
 */

import { createWriteStream, mkdirSync } from 'node:fs';
import path from 'node:path';

const FAR_FUTURE_MS = 4_102_444_800_000; // year 2100, the ceiling SimulationConfig::whole_lake uses
const MIN_MS = 1_000_000_000_000; // 2001-09-09, the same floor the lake validates against

function usage(message) {
  if (message) console.error(`error: ${message}\n`);
  console.error(`Usage: node record_live_ws.mjs [OPTIONS]

  --data <DIR>          lake directory to write into            (default ./data)
  --market um|spot      USDⓈ-M futures or spot stream host      (default um)
  --symbols <A,B,...>   symbols to record                        (default BTCUSDT)
  --book depth20|bookTicker  book source: 20 price levels at 100 ms, or best bid/ask
                                                               (default depth20)
  --minutes <N>         stop after N minutes; 0 runs until Ctrl-C (default 0)
  --rotate-minutes <N>  start a new .jsonl file every N minutes  (default 10)
  --progress <N>        print a line every N events             (default 5000)
  --help                this text

Files land as events-<UTC stamp>.jsonl so a long capture is several lake-readable files
rather than one growing one: the replay orders events by timestamp across all of them.`);
  process.exit(message ? 1 : 0);
}

function parseArgs(argv) {
  const options = {
    data: process.env.AEGIS_DATA_PATH || './data',
    market: 'um',
    symbols: ['BTCUSDT'],
    book: 'depth20',
    minutes: 0,
    rotateMinutes: 10,
    progress: 5000,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = () => {
      i += 1;
      if (i >= argv.length) usage(`${flag} expects a value`);
      return argv[i];
    };
    switch (flag) {
      case '--help':
      case '-h':
        usage();
        break;
      case '--data':
        options.data = value();
        break;
      case '--market':
        options.market = value().toLowerCase();
        break;
      case '--symbols':
        options.symbols = value()
          .split(',')
          .map((s) => s.trim().toUpperCase())
          .filter(Boolean);
        break;
      case '--book':
        options.book = value().toLowerCase();
        break;
      case '--minutes':
        options.minutes = Number(value());
        break;
      case '--rotate-minutes':
        options.rotateMinutes = Number(value());
        break;
      case '--progress':
        options.progress = Number(value());
        break;
      default:
        usage(`unknown option ${flag}`);
    }
  }
  if (!['um', 'spot'].includes(options.market)) usage("--market must be um or spot");
  if (options.symbols.length === 0) usage('--symbols needs at least one symbol');
  if (!['depth20', 'bookticker'].includes(options.book)) {
    usage('--book must be depth20 or bookTicker');
  }
  for (const key of ['minutes', 'rotateMinutes', 'progress']) {
    if (!Number.isFinite(options[key]) || options[key] < 0) usage(`--${key} expects a number >= 0`);
  }
  if (options.rotateMinutes === 0) usage('--rotate-minutes must be greater than zero');
  options.book = options.book === 'bookticker' ? 'bookTicker' : 'depth20';
  return options;
}

const options = parseArgs(process.argv.slice(2));
const wanted = new Set(options.symbols);
const lower = options.symbols.map((s) => s.toLowerCase());

const streams = [];
for (const symbol of lower) {
  // The partial book stream is a snapshot of the top levels, which is what the replay
  // needs - a diff stream would require rebuilding a book from deltas, a different job.
  streams.push(options.book === 'depth20' ? `${symbol}@depth20@100ms` : `${symbol}@bookTicker`);
  streams.push(options.market === 'um' ? `${symbol}@aggTrade` : `${symbol}@trade`);
  if (options.market === 'um') streams.push(`${symbol}@markPrice@1s`);
}

const host =
  options.market === 'um' ? 'fstream.binance.com' : 'data-stream.binance.vision';
const url = `wss://${host}/stream?streams=${streams.join('/')}`;

mkdirSync(options.data, { recursive: true });

const totals = {
  received: 0,
  written: 0,
  rejected: 0,
  malformed: 0,
  reconnects: 0,
  files: 0,
};

let stream = null;
let openedAt = 0;
let stopping = false;

function stamp() {
  return new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d+Z$/, 'Z');
}

function openFile() {
  const file = path.join(options.data, `events-${stamp()}.jsonl`);
  stream = createWriteStream(file, { flags: 'a' });
  openedAt = Date.now();
  totals.files += 1;
  console.log(`recording into ${file}`);
  stream.on('error', (error) => {
    console.error(`write error on ${file}: ${error.message}`);
  });
  return file;
}

let currentFile = openFile();

function rotateIfNeeded() {
  const age = Date.now() - openedAt;
  if (age >= options.rotateMinutes * 60_000) {
    const closed = currentFile;
    stream.end(() => console.log(`closed ${closed}`));
    stream = null;
    currentFile = openFile();
  }
}

function writeEvent(record) {
  if (stream === null) return;
  stream.write(`${JSON.stringify(record)}\n`);
  totals.written += 1;
}

function msTimestamp(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return null;
  const millis = Math.round(number);
  if (millis < MIN_MS || millis > FAR_FUTURE_MS) return null;
  return millis;
}

function priceLevels(rows) {
  if (!Array.isArray(rows)) return [];
  const levels = [];
  for (const row of rows) {
    const price = Number(Array.isArray(row) ? row[0] : row?.p);
    const quantity = Number(Array.isArray(row) ? row[1] : row?.q);
    // A zero quantity is a removed level, not a free bid: keeping it would let the
    // imbalance feature count liquidity that is not in the book.
    if (Number.isFinite(price) && price > 0 && Number.isFinite(quantity) && quantity > 0) {
      levels.push([price, quantity]);
    }
  }
  return levels;
}

function handle(data) {
  totals.received += 1;
  if (options.progress > 0 && totals.received % options.progress === 0) {
    console.log(
      `events ${totals.received}, written ${totals.written}, rejected ${totals.rejected}`
    );
  }

  const symbol = typeof data.s === 'string' ? data.s.toUpperCase() : null;
  if (symbol === null || !wanted.has(symbol)) return;

  const timestamp = msTimestamp(data.E ?? data.T);
  if (timestamp === null) {
    totals.rejected += 1;
    return;
  }

  switch (data.e) {
    case 'depthUpdate': {
      const bids = priceLevels(data.b ?? data.bids);
      const asks = priceLevels(data.a ?? data.asks);
      // A book with one side empty is not a book; the reader would reject the row anyway,
      // and a rejected row is a worse outcome than an honest skip with a counter.
      if (bids.length === 0 || asks.length === 0) {
        totals.rejected += 1;
        return;
      }
      writeEvent({ symbol, timestamp, bids, asks });
      return;
    }
    case 'aggTrade':
    case 'trade': {
      const price = Number(data.p);
      const quantity = Number(data.q);
      if (!(price > 0) || !(quantity > 0)) {
        totals.rejected += 1;
        return;
      }
      writeEvent({
        symbol,
        timestamp,
        price,
        quantity,
        // Recorded as the venue reports it. `is_buyer_maker: true` means the *buyer* was
        // the resting side, so the aggressor was a sell - the reader inverts this, and a
        // test in the lake pins that inversion.
        is_buyer_maker: data.m === true,
      });
      return;
    }
    case 'markPriceUpdate': {
      const rate = Number(data.r);
      if (!Number.isFinite(rate)) {
        totals.rejected += 1;
        return;
      }
      // The mark-price stream carries the *predicted* funding rate between settlements. The
      // replay charges it as funding, which is the conservative reading: funding is being
      // paid the whole interval, not only at the hour.
      writeEvent({ symbol, timestamp, funding_rate: rate });
      return;
    }
    default:
      return;
  }
}

let socket = null;

function connect() {
  if (stopping) return;
  socket = new WebSocket(url);

  socket.addEventListener('open', () => {
    console.log(`connected to ${host} (${streams.length} streams)`);
  });

  socket.addEventListener('message', (event) => {
    if (typeof event.data !== 'string') return;
    let envelope;
    try {
      envelope = JSON.parse(event.data);
    } catch {
      totals.malformed += 1;
      return;
    }
    // A combined stream wraps each payload as {stream, data}; a bare socket would be a
    // single payload object, so accept both.
    handle(envelope.data ?? envelope);
    rotateIfNeeded();
  });

  socket.addEventListener('error', (event) => {
    console.error(`socket error: ${event.message ?? 'see the close event'}`);
  });

  socket.addEventListener('close', () => {
    if (stopping) return;
    totals.reconnects += 1;
    const delay = Math.min(15_000, 500 * 2 ** Math.min(totals.reconnects, 5));
    console.log(`socket closed; reconnecting in ${delay} ms`);
    setTimeout(connect, delay).unref?.();
  });
}

function finish(signal) {
  if (stopping) return;
  stopping = true;
  console.log(`\n${signal}: closing the recorder`);
  try {
    socket?.close();
  } catch {
    /* the socket may already be gone */
  }
  const done = () => {
    console.log(
      `received ${totals.received}, written ${totals.written} event(s) across ${totals.files} file(s), ` +
        `rejected ${totals.rejected}, unparseable ${totals.malformed}, reconnects ${totals.reconnects}`
    );
    if (totals.written === 0) {
      console.log(
        'nothing was recorded: the replay will report the lake as empty, which is the honest result'
      );
    } else {
      console.log(`next: cargo run --release -- --backtest --data ${options.data}`);
    }
  };
  if (stream) stream.end(done);
  else done();
}

process.on('SIGINT', () => finish('SIGINT'));
process.on('SIGTERM', () => finish('SIGTERM'));

if (options.minutes > 0) {
  setTimeout(() => finish(`after ${options.minutes} minute(s)`), options.minutes * 60_000).unref?.();
}

connect();
