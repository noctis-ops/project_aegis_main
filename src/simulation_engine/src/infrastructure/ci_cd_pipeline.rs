//! CI/CD Pipeline implementation
//!
//! Every gate here has to be able to say "no". `run_unit_tests` used to return a
//! hardcoded `true` and `run_smoke_backtest` a fabricated 5%/50-trade report, so a build
//! with failing tests and no data would have reported green - which in a trading system
//! is not a cosmetic problem, it is the alarm that does not go off. The gates now execute
//! the real commands and replay the real data lake, and they fail when the environment
//! cannot be verified.

use crate::components::{BacktestingEngine, CapitalAdapter, DataLake};
use crate::core::{BacktestReport, SimulationConfig, SimulationError};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;
use tracing::{error, info};

/// CI/CD Pipeline for automated testing and deployment
pub struct CiCdPipeline {
    github_repo: String,
    aws_region: String,
    is_initialized: bool,
}

impl CiCdPipeline {
    /// Create a new CI/CD Pipeline
    pub fn new() -> Self {
        Self {
            github_repo: "project-aegis/hft-bot".to_string(),
            aws_region: crate::core::constants::AWS_REGION.to_string(),
            is_initialized: false,
        }
    }

    /// Initialize the CI/CD pipeline
    pub fn initialize(&mut self, github_repo: &str, aws_region: &str) -> Result<(), SimulationError> {
        if github_repo.trim().is_empty() || aws_region.trim().is_empty() {
            return Err(SimulationError::PipelineError(
                "CI/CD needs both a GitHub repository and an AWS region".to_string(),
            ));
        }

        self.github_repo = github_repo.to_string();
        self.aws_region = aws_region.to_string();
        self.is_initialized = true;

        info!("CI/CD Pipeline initialized for repo: {} in region: {}", github_repo, aws_region);
        Ok(())
    }

    /// Run the crate's unit tests with the release profile (the profile the bot ships
    /// with: optimization changes floating-point behaviour, so tests are run there too).
    pub fn run_unit_tests(&self) -> Result<bool, SimulationError> {
        self.require_initialized()?;
        info!("Running unit tests...");

        match Self::run("test", &["test", "--release"]) {
            Ok(output) if output.status.success() => {
                info!("Unit tests PASSED");
                Ok(true)
            }
            Ok(output) => {
                error!(
                    "Unit tests FAILED ({}): {}",
                    output
                        .status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "signal".to_string()),
                    tail_str(&String::from_utf8_lossy(&output.stderr), 40)
                );
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    /// Run smoke backtest over the last hour of the configured data lake.
    ///
    /// The report it returns is measured: with no recorded data in the window it errors,
    /// and that is the correct CI signal, not a green light.
    pub async fn run_smoke_backtest(&self) -> Result<BacktestReport, SimulationError> {
        let config = Self::smoke_config(Utc::now());
        self.run_smoke_backtest_for(&config).await
    }

    /// Smoke backtest against an explicit configuration (used by tests and by a pipeline
    /// whose lake lives outside the default path).
    pub async fn run_smoke_backtest_for(
        &self,
        config: &SimulationConfig,
    ) -> Result<BacktestReport, SimulationError> {
        self.require_initialized()?;
        info!("Running smoke backtest...");

        let report = self.replay(config).await?;

        info!(
            "Smoke backtest measured {:+.2}% over {} fill(s) (drawdown {:.2}%)",
            report.results.total_return * 100.0,
            report.results.total_trades,
            report.results.max_drawdown * 100.0
        );
        Ok(report)
    }

    /// Build release binary
    pub fn build_release(&self) -> Result<(), SimulationError> {
        self.require_initialized()?;
        info!("Building release binary...");

        match Self::run("build", &["build", "--release"]) {
            Ok(output) if output.status.success() => {
                info!("Release build completed successfully");
                Ok(())
            }
            Ok(output) => {
                let detail = tail_str(&String::from_utf8_lossy(&output.stderr), 40);
                error!("Release build failed: {}", detail);
                Err(SimulationError::PipelineError(format!(
                    "Build failed: {}",
                    detail
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Run the full test matrix: every capital/leverage case the capital adapter
    /// prescribes, replayed over the same data and judged against its own acceptance
    /// criteria.
    pub async fn run_test_matrix(
        &self,
        base_config: &SimulationConfig,
    ) -> Result<Vec<BacktestReport>, SimulationError> {
        self.require_initialized()?;
        info!("Running full test matrix...");

        let mut lake = DataLake::new();
        lake.initialize(&base_config.data_path)?;
        let events = lake.collect_events(base_config)?;

        let adapter = CapitalAdapter::new();
        let engine = BacktestingEngine::new();
        let mut reports = Vec::new();

        for case in adapter.generate_test_cases() {
            let mut config = base_config.clone();
            config.initial_capital = case.capital;
            config.leverage = case.leverage;

            let report = engine
                .run_simulation(&config, &events)
                .await
                .map_err(|e| {
                    SimulationError::PipelineError(format!(
                        "Matrix case ${:.0} at {:.0}x could not be replayed: {}",
                        case.capital, case.leverage, e
                    ))
                })?;

            if !adapter.validate_test_result(&report, &case) {
                return Err(SimulationError::PipelineError(format!(
                    "Matrix case ${:.0} at {:.0}x failed its acceptance criteria \
                     (return {:+.2}%, drawdown {:.2}%, execution quality {:.1}%)",
                    case.capital,
                    case.leverage,
                    report.results.total_return * 100.0,
                    report.results.max_drawdown * 100.0,
                    report.results.execution_quality * 100.0
                )));
            }

            reports.push(report);
        }

        info!("Test matrix completed: {} case(s) passed", reports.len());
        Ok(reports)
    }

    /// The window a smoke run covers: an hour of the newest data in the lake.
    fn smoke_config(now: DateTime<Utc>) -> SimulationConfig {
        SimulationConfig {
            end_time: now,
            start_time: now - chrono::Duration::hours(1),
            playback_speed: 10.0, // Fast playback for smoke test
            initial_capital: 1_000.0,
            ..SimulationConfig::default()
        }
    }

    async fn replay(&self, config: &SimulationConfig) -> Result<BacktestReport, SimulationError> {
        let mut lake = DataLake::new();
        lake.initialize(&config.data_path)?;
        let events = lake.collect_events(config)?;

        BacktestingEngine::new().run_simulation(config, &events).await
    }

    fn require_initialized(&self) -> Result<(), SimulationError> {
        if !self.is_initialized {
            return Err(SimulationError::PipelineError(
                "CI/CD pipeline not initialized; a gate that never ran cannot report a pass".to_string(),
            ));
        }
        Ok(())
    }

    /// Run cargo with `args` from the crate's own directory, so the gate is not at the
    /// mercy of whatever directory the binary happened to be launched from.
    fn run(label: &str, args: &[&str]) -> Result<std::process::Output, SimulationError> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

        let output = Command::new("cargo")
            .args(args)
            .current_dir(manifest_dir)
            .output()
            .map_err(|e| {
                SimulationError::PipelineError(format!(
                    "Cannot run 'cargo {}' in {}: {} - the gate cannot verify anything, so it fails",
                    label,
                    manifest_dir.display(),
                    e
                ))
            })?;

        Ok(output)
    }

    /// Check if pipeline is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

/// The last `lines` of a command's output, for a log that stays readable when cargo
/// prints a hundred diagnostics.
fn tail_str(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    if all.len() <= lines {
        return all.join("\n");
    }
    all[all.len() - lines..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_uninitialized_pipeline_cannot_report_a_pass() {
        let pipeline = CiCdPipeline::new();

        assert!(matches!(
            pipeline.run_unit_tests(),
            Err(SimulationError::PipelineError(_))
        ));
        assert!(matches!(
            pipeline.build_release(),
            Err(SimulationError::PipelineError(_))
        ));
    }

    #[test]
    fn empty_pipeline_settings_are_refused() {
        let mut pipeline = CiCdPipeline::new();
        assert!(pipeline.initialize("", "eu-west-1").is_err());
        assert!(pipeline.initialize("aegis/bot", "  ").is_err());
        assert!(pipeline.initialize("aegis/bot", "eu-west-1").is_ok());
        assert!(pipeline.is_initialized());
    }

    #[test]
    fn the_smoke_window_is_the_last_hour_at_small_capital() {
        let now = Utc::now();
        let config = CiCdPipeline::smoke_config(now);

        assert_eq!((now - config.start_time).num_seconds(), 3_600);
        assert_eq!(config.end_time, now);
        assert_eq!(config.initial_capital, 1_000.0);
        assert_eq!(config.playback_speed, 10.0);
        // The strategy knobs and fees come from the defaults, so the smoke run tests the
        // same rule the live path trades.
        assert_eq!(
            config.strategy.obi_threshold,
            SimulationConfig::default().strategy.obi_threshold
        );
    }

    #[test]
    fn cargo_output_is_trimmed_not_dumped() {
        let text = (0..100)
            .map(|index| format!("line {}", index))
            .collect::<Vec<_>>()
            .join("\n");

        let trimmed = tail_str(&text, 3);
        assert_eq!(trimmed, "line 97\nline 98\nline 99");
        assert_eq!(tail_str("one\n", 5), "one");
        assert_eq!(tail_str("", 5), "");
    }

    #[tokio::test]
    async fn a_smoke_run_over_a_lake_with_no_data_fails_instead_of_passing() {
        let directory = std::env::temp_dir().join(format!("aegis_ci_empty_{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");

        let mut pipeline = CiCdPipeline::new();
        pipeline
            .initialize("aegis/bot", crate::core::constants::AWS_REGION)
            .expect("settings");

        let mut config = CiCdPipeline::smoke_config(Utc::now());
        config.data_path = directory.to_str().expect("utf-8 temp path").to_string();

        let error = pipeline
            .run_smoke_backtest_for(&config)
            .await
            .expect_err("an empty lake is not a passing smoke test");
        assert!(error.to_string().contains("No event files"), "{}", error);

        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test]
    async fn a_smoke_run_reports_what_the_replay_measured() {
        let directory = std::env::temp_dir().join(format!("aegis_ci_smoke_{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");

        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        let rows = [
            format!(
                r#"{{"symbol":"BTCUSDT","timestamp":{},"bids":[[40000.0,8.0]],"asks":[[40001.0,0.5]]}}"#,
                now_ms - 60_000
            ),
            format!(
                r#"{{"symbol":"BTCUSDT","timestamp":{},"price":40000.0,"quantity":1.0,"side":"sell"}}"#,
                now_ms - 59_000
            ),
            format!(
                r#"{{"symbol":"BTCUSDT","timestamp":{},"price":40400.5,"quantity":1.0,"side":"buy"}}"#,
                now_ms - 30_000
            ),
        ]
        .join("\n");
        std::fs::write(directory.join("events.jsonl"), rows).expect("fixture written");

        let mut pipeline = CiCdPipeline::new();
        pipeline
            .initialize("aegis/bot", crate::core::constants::AWS_REGION)
            .expect("settings");

        let mut config = CiCdPipeline::smoke_config(now);
        config.data_path = directory.to_str().expect("utf-8 temp path").to_string();

        let report = pipeline
            .run_smoke_backtest_for(&config)
            .await
            .expect("the fixture is inside the smoke window");

        assert_eq!(report.results.total_trades, 2, "one entry and one exit");
        assert!(report.results.total_return > 0.0, "{:?}", report.results);
        // Not the fabricated 5% the mock reported.
        assert!(report.results.total_return < 0.05, "{:?}", report.results);

        std::fs::remove_dir_all(&directory).ok();
    }
}
