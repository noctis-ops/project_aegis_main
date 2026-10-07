//! Project AEGIS - Layer 5
//! Backtesting Infrastructure, Simulation & CI/CD Deployment
//!
//! Thin launcher: the entire engine lives in the library crate, so this binary parses a
//! run, calls the engine and maps the outcome to an exit code. Declaring the modules here
//! again (as before) made cargo compile the whole layer twice - every error in it was
//! reported twice, with different warnings per target - which is exactly what Layers 1 to
//! 4 stopped doing.
//!
//! Modes, and why each one exists:
//!
//! * `backtest` (default) - replay the recorded events over a window and print what the
//!   replay measured. A run that cannot measure something fails instead of reporting zero.
//! * `robustness` - the reference replay first, then each swept knob one step at a time,
//!   so the spread across reports is attributable to the parameter and not to the feed
//!   (`SimulationEngine::run_robustness_tests`).
//! * `capital-matrix` - the capital/leverage matrix over the same window, each case
//!   validated against what that run actually booked.
//! * `serve` - start the engine's tasks and park, which is what an external feeder
//!   (Layers 1 to 4, or the Telegram C2) needs. Nothing is measured in this mode.
//!
//! The window defaults to everything the lake holds (`SimulationConfig::whole_lake`), so
//! a run without `--start`/`--end` measures the files instead of an empty hour around
//! `Utc::now()`.

use chrono::{DateTime, Utc};
use simulation_engine::core::{SimulationConfig, SimulationError};
use simulation_engine::{BacktestReport, SimulationEngine};
use std::path::Path;
use std::process::ExitCode;
use tracing::{error, info};

/// A run that never ends is not a pass: the exit code is how a shell, a scheduled task or
/// a CI step learns whether a measurement happened at all.
const USAGE: &str = "\
Usage: simulation-engine [MODE] [OPTIONS]

Modes:
  --backtest            replay the data lake and print the report (default)
  --robustness          reference replay, then each swept knob one step at a time
  --capital-matrix      capital/leverage matrix over the same window
  --serve               start the engine tasks and stay alive for an external feeder
  --help, -h            print this text

Options (each one also reads the environment variable named under it):
  --data <DIR>          lake directory                        AEGIS_DATA_PATH  (./data)
  --start <RFC3339>     window start                          AEGIS_START      (whole lake)
  --end <RFC3339>       window end                            AEGIS_END        (whole lake)
  --symbols <A,B>       comma separated                       AEGIS_SYMBOLS    (BTCUSDT)
  --capital <f64>       starting capital, USDT                AEGIS_CAPITAL    (10000)
  --leverage <f64>      multiplier                            AEGIS_LEVERAGE   (10)
  --maker-fee <f64>     per-position rate, 0.0002 = 0.02%     AEGIS_MAKER_FEE  (0.0002)
  --taker-fee <f64>     per-position rate                     AEGIS_TAKER_FEE  (0.0004)
  --obi-threshold <f64>  imbalance needed to enter           AEGIS_OBI_THRESHOLD (0.4)
  --obi-levels <N>      depth the imbalance is measured over  AEGIS_OBI_LEVELS (5)
  --risk-percentage <f64> share of capital risked per entry   AEGIS_RISK_PCT   (0.01)
  --volatility-window <N> prints kept for the volatility gate AEGIS_VOL_WINDOW (100)
  --max-volatility <f64>  0 disables the volatility gate      AEGIS_MAX_VOL    (0)
  --size-step <f64>     venue quantity step                   AEGIS_SIZE_STEP  (0.001)
  --max-position-value <f64> notional cap, USDT               AEGIS_MAX_POSITION_VALUE (10000)
  --report <FILE.json>  also write the report(s) to a file    AEGIS_REPORT_PATH

A live backend is never armed from here. The shadow ledger is the only execution this
binary can request; `/mode live` on the Telegram C2 still needs its single-use code, and
arming real orders remains an explicit act in code.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Backtest,
    Robustness,
    Capital,
    Serve,
    Help,
}

struct Options {
    mode: Mode,
    config: SimulationConfig,
    report_path: Option<String>,
}

impl Options {
    /// Environment first, flags on top: a scheduled task sets the run once through the
    /// environment, and an operator at a prompt overrides one field without retyping the
    /// rest. A malformed value is a hard error rather than a silent fallback, because a
    /// backtest run at the wrong capital is still reported as a result.
    fn parse<I: Iterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut config = SimulationConfig::whole_lake(env_string("AEGIS_DATA_PATH", "./data"));

        config.initial_capital = env_number("AEGIS_CAPITAL", config.initial_capital)?;
        config.leverage = env_number("AEGIS_LEVERAGE", config.leverage)?;
        config.maker_fee_rate = env_number("AEGIS_MAKER_FEE", config.maker_fee_rate)?;
        config.taker_fee_rate = env_number("AEGIS_TAKER_FEE", config.taker_fee_rate)?;
        config.symbols = split_symbols(&env_string("AEGIS_SYMBOLS", "BTCUSDT"))?;
        config.strategy.obi_threshold =
            env_number("AEGIS_OBI_THRESHOLD", config.strategy.obi_threshold)?;
        config.strategy.obi_levels = env_usize("AEGIS_OBI_LEVELS", config.strategy.obi_levels)?;
        config.strategy.volatility_window =
            env_usize("AEGIS_VOL_WINDOW", config.strategy.volatility_window)?;
        config.strategy.risk_percentage =
            env_number("AEGIS_RISK_PCT", config.strategy.risk_percentage)?;
        config.strategy.max_volatility =
            env_number("AEGIS_MAX_VOL", config.strategy.max_volatility)?;
        config.strategy.size_step = env_number("AEGIS_SIZE_STEP", config.strategy.size_step)?;
        config.strategy.max_position_value =
            env_number("AEGIS_MAX_POSITION_VALUE", config.strategy.max_position_value)?;
        config.start_time = env_time("AEGIS_START", config.start_time)?;
        config.end_time = env_time("AEGIS_END", config.end_time)?;

        let mut mode = Mode::Backtest;
        let mut report_path = std::env::var("AEGIS_REPORT_PATH")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let args: Vec<String> = args.collect();
        let mut index = 0usize;
        while index < args.len() {
            let flag = args[index].as_str();
            index += 1;

            // Every value-taking arm consumes the next argument through `take_value`,
            // which hands back an owned String: a helper borrowing out of `args` while the
            // loop indexes it is the borrow the checker rejects, and cloning a CLI token is
            // free next to reading a data lake.
            match flag {
                "--backtest" => mode = Mode::Backtest,
                "--robustness" => mode = Mode::Robustness,
                "--capital-matrix" => mode = Mode::Capital,
                "--serve" => mode = Mode::Serve,
                "--help" | "-h" => mode = Mode::Help,
                "--data" => config.data_path = take_value(&args, &mut index, flag)?,
                "--start" => config.start_time = parse_time(&take_value(&args, &mut index, flag)?, flag)?,
                "--end" => config.end_time = parse_time(&take_value(&args, &mut index, flag)?, flag)?,
                "--symbols" => config.symbols = split_symbols(&take_value(&args, &mut index, flag)?)?,
                "--capital" => config.initial_capital = parse_number(&take_value(&args, &mut index, flag)?, flag)?,
                "--leverage" => config.leverage = parse_number(&take_value(&args, &mut index, flag)?, flag)?,
                "--maker-fee" => config.maker_fee_rate = parse_number(&take_value(&args, &mut index, flag)?, flag)?,
                "--taker-fee" => config.taker_fee_rate = parse_number(&take_value(&args, &mut index, flag)?, flag)?,
                "--obi-threshold" => {
                    config.strategy.obi_threshold = parse_number(&take_value(&args, &mut index, flag)?, flag)?
                }
                "--obi-levels" => config.strategy.obi_levels = parse_usize(&take_value(&args, &mut index, flag)?, flag)?,
                "--volatility-window" => {
                    config.strategy.volatility_window = parse_usize(&take_value(&args, &mut index, flag)?, flag)?
                }
                "--risk-percentage" => {
                    config.strategy.risk_percentage = parse_number(&take_value(&args, &mut index, flag)?, flag)?
                }
                "--max-volatility" => {
                    config.strategy.max_volatility = parse_number(&take_value(&args, &mut index, flag)?, flag)?
                }
                "--size-step" => config.strategy.size_step = parse_number(&take_value(&args, &mut index, flag)?, flag)?,
                "--max-position-value" => {
                    config.strategy.max_position_value = parse_number(&take_value(&args, &mut index, flag)?, flag)?
                }
                "--report" => report_path = Some(take_value(&args, &mut index, flag)?),
                other => return Err(format!("unknown option `{other}` (try --help)")),
            }
        }

        if config.symbols.is_empty() {
            return Err("--symbols needs at least one symbol, e.g. --symbols BTCUSDT".to_string());
        }
        if config.start_time >= config.end_time {
            return Err(format!(
                "the window is empty: --start {} is not before --end {}",
                config.start_time.to_rfc3339(),
                config.end_time.to_rfc3339()
            ));
        }

        Ok(Self {
            mode,
            config,
            report_path,
        })
    }
}

fn take_value(args: &[String], index: &mut usize, name: &str) -> Result<String, String> {
    let raw = args
        .get(*index)
        .ok_or_else(|| format!("{name} expects a value"))?
        .clone();
    *index += 1;
    Ok(raw)
}

fn env_string(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn env_number(key: &str, fallback: f64) -> Result<f64, String> {
    match std::env::var(key) {
        Err(_) => Ok(fallback),
        Ok(raw) if raw.trim().is_empty() => Ok(fallback),
        Ok(raw) => parse_number(raw.trim(), key),
    }
}

fn env_usize(key: &str, fallback: usize) -> Result<usize, String> {
    match std::env::var(key) {
        Err(_) => Ok(fallback),
        Ok(raw) if raw.trim().is_empty() => Ok(fallback),
        Ok(raw) => parse_usize(raw.trim(), key),
    }
}

fn env_time(key: &str, fallback: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    match std::env::var(key) {
        Err(_) => Ok(fallback),
        Ok(raw) if raw.trim().is_empty() => Ok(fallback),
        Ok(raw) => parse_time(raw.trim(), key),
    }
}

fn parse_number(raw: &str, key: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .map_err(|_| format!("{key} expects a number, got `{raw}`"))
}

fn parse_usize(raw: &str, key: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|_| format!("{key} expects a whole number, got `{raw}`"))
}

fn parse_time(raw: &str, key: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|moment| moment.with_timezone(&Utc))
        .map_err(|_| {
            format!(
                "{key} expects an RFC 3339 timestamp such as 2026-10-01T00:00:00Z, got `{raw}`"
            )
        })
}

fn split_symbols(raw: &str) -> Result<Vec<String>, String> {
    let symbols: Vec<String> = raw
        .split(',')
        .map(|symbol| symbol.trim().to_ascii_uppercase())
        .filter(|symbol| !symbol.is_empty())
        .collect();
    if symbols.is_empty() {
        return Err(format!("no symbol found in `{raw}`"));
    }
    Ok(symbols)
}

fn print_report(report: &BacktestReport) {
    let results = &report.results;
    println!("--- AEGIS replay ------------------------------------------------");
    println!(
        "window            {} .. {}",
        report.config.start_time.to_rfc3339(),
        report.config.end_time.to_rfc3339()
    );
    println!("symbols           {}", report.config.symbols.join(", "));
    println!(
        "capital           {:.2} USDT at {:.1}x",
        report.config.initial_capital, report.config.leverage
    );
    println!(
        "fees              maker {:.4}%, taker {:.4}%",
        report.config.maker_fee_rate * 100.0,
        report.config.taker_fee_rate * 100.0
    );
    println!(
        "strategy          obi > {:.3} over {} level(s), risk {:.2}% of capital",
        report.config.strategy.obi_threshold,
        report.config.strategy.obi_levels,
        report.config.strategy.risk_percentage * 100.0
    );
    println!("--------------------------------------- measured, not asserted ---");
    println!("total return      {:+.3}%", results.total_return * 100.0);
    println!("sharpe            {:.3}", results.sharpe_ratio);
    println!("max drawdown      {:.3}%", results.max_drawdown * 100.0);
    println!("win rate          {:.1}%", results.win_rate * 100.0);
    println!("profit factor     {:.3}", results.profit_factor);
    println!("closed trades     {}", results.total_trades);
    println!("avg round trip    {:.3} s", results.avg_trade_duration);
    println!("slippage impact   {:.6}", results.slippage_impact);
    println!(
        "execution quality {:.1}% of submitted orders filled",
        results.execution_quality * 100.0
    );
    println!(
        "detail            {} equity point(s), {} trade log entry(s)",
        report.equity_curve.len(),
        report.trade_log.len()
    );
    println!("-------------------------------------------------------------------");
}

fn write_report(path: &str, reports: &[BacktestReport]) -> Result<(), String> {
    // One report stays an object; a matrix stays an array. A file the dashboard reads
    // should not change shape depending on how many cases were asked for.
    let document = if reports.len() == 1 {
        serde_json::to_string_pretty(&reports[0])
    } else {
        serde_json::to_string_pretty(reports)
    }
    .map_err(|e| format!("could not serialize the report: {e}"))?;

    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
    }

    std::fs::write(path, document).map_err(|e| format!("could not write {path}: {e}"))
}

/// A matrix is printed case by case, then handed back for the file. The printing lives
/// here, in the binary, so the library keeps producing numbers rather than a layout.
fn print_matrix(label: &str, reports: &[BacktestReport]) {
    println!("--- AEGIS {label} -------------------------------------------------");
    for report in reports {
        println!(
            "{:>12.2} USDT at {:>4.1}x : {:+.3}% return, {:.1}% drawdown, {} closed trade(s), {:.1}% of orders filled",
            report.config.initial_capital,
            report.config.leverage,
            report.results.total_return * 100.0,
            report.results.max_drawdown * 100.0,
            report.results.total_trades,
            report.results.execution_quality * 100.0
        );
    }
    println!("{} case(s) measured", reports.len());
}

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize structured logging
    tracing_subscriber::fmt::init();

    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n");
            eprintln!("{}", USAGE);
            return ExitCode::FAILURE;
        }
    };

    if options.mode == Mode::Help {
        println!("{}", USAGE);
        return ExitCode::SUCCESS;
    }

    info!("Starting Project AEGIS - Layer 5 Simulation Engine");

    let mut engine = SimulationEngine::new();
    info!(
        "Active execution backend: {:?} (a live backend has to be armed explicitly)",
        engine.execution_mode()
    );

    if options.mode == Mode::Serve {
        if let Err(e) = engine.start().await {
            error!("Simulation engine terminated with error: {}", e);
            return ExitCode::FAILURE;
        }
        info!("Project AEGIS - Layer 5 Simulation Engine started, parking main task");

        // The simulation runs on the tasks `start()` spawned; nothing here to do but stay
        // alive. `start()` returning Ok never means "finished", so exiting would kill the
        // backtest mid-flight.
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }

    // Every measured mode starts from the same reference replay, so the window, symbols
    // and fees a matrix was judged under are the ones printed above it. `run_backtest`
    // also adopts the configuration, which is what the matrix functions then perturb.
    let reference = match engine.run_backtest(options.config.clone()).await {
        Ok(report) => report,
        Err(e) => {
            error!("Replay failed: {}", describe(&e));
            return ExitCode::FAILURE;
        }
    };
    print_report(&reference);

    let outcome: Result<Vec<BacktestReport>, SimulationError> = match options.mode {
        Mode::Robustness => engine.run_robustness_tests().await,
        Mode::Capital => engine.run_capital_adaptation_tests().await,
        _ => Ok(vec![reference.clone()]),
    };

    let reports = match outcome {
        Ok(reports) => reports,
        Err(e) => {
            error!("{} run failed: {}", mode_name(options.mode), describe(&e));
            return ExitCode::FAILURE;
        }
    };

    if matches!(options.mode, Mode::Robustness | Mode::Capital) {
        print_matrix(mode_name(options.mode), &reports);
    }

    if let Some(path) = &options.report_path {
        if let Err(e) = write_report(path, &reports) {
            error!("Report not written: {e}");
            return ExitCode::FAILURE;
        }
        info!("Report written to {}", path);
    }

    println!(
        "AEGIS {mode} completed with {cases} report(s): the figures above are measured \
         from recorded events, which is the only kind of evidence a parameter change \
         should be trusted with real capital over.",
        mode = mode_name(options.mode),
        cases = reports.len()
    );
    ExitCode::SUCCESS
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Backtest => "backtest",
        Mode::Robustness => "robustness",
        Mode::Capital => "capital",
        Mode::Serve => "serve",
        Mode::Help => "help",
    }
}

/// `Debug` prints the variant name next to the message, which is what makes a log line
/// greppable when a run failed for one of ten reasons.
fn describe(error: &SimulationError) -> String {
    format!("{error:?}")
}
