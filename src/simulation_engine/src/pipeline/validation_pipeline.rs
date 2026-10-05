//! Validation pipeline for robustness and overfitting testing
//!
//! Both tests here run the *real* replay: the data lake is read once, every parameter
//! variant is replayed through [`BacktestingEngine`] against the same events, and the
//! resulting reports are judged. The mock backtest this file used to carry returned a
//! constant 12% return / Sharpe 2.1 / 6% drawdown, so the comparison below divided
//! identical numbers by each other, the ratio came out 1.0, and every strategy passed -
//! including a strategy that does not exist.

use crate::components::{BacktestingEngine, DataLake};
use crate::core::constants::{MAX_ACCEPTABLE_PARAMETER_DEGRADATION, OVERFITTING_TEST_RANGE};
use crate::core::{BacktestReport, MarketEvent, RobustnessTestParams, SimulationConfig, SimulationError};
use tracing::{debug, error, info, warn};

/// Validation pipeline for robustness and overfitting testing
pub struct ValidationPipeline {
    robustness_params: RobustnessTestParams,
    is_configured: bool,
}

impl ValidationPipeline {
    /// Create a new validation pipeline
    pub fn new() -> Self {
        Self {
            robustness_params: RobustnessTestParams {
                parameter_ranges: vec![],
                test_points: 0,
                min_acceptable_performance: 0.0,
            },
            is_configured: false,
        }
    }

    /// Configure validation parameters
    pub fn configure(&mut self, params: RobustnessTestParams) -> Result<(), SimulationError> {
        // `run_robustness_tests` divides the range by `test_points`, so a zero here is
        // not just a divide-by-zero: the sweep would silently run no test at all and
        // still report an empty - but successful - robustness result.
        if params.test_points == 0 {
            return Err(SimulationError::InvalidConfig(
                "Robustness testing needs at least one test point".to_string(),
            ));
        }
        if params.parameter_ranges.is_empty() {
            return Err(SimulationError::InvalidConfig(
                "Robustness testing needs at least one parameter range to sweep".to_string(),
            ));
        }
        for (name, min_value, max_value) in &params.parameter_ranges {
            if !min_value.is_finite() || !max_value.is_finite() || max_value < min_value {
                return Err(SimulationError::InvalidConfig(format!(
                    "Range [{}, {}] for '{}' is not a usable interval",
                    min_value, max_value, name
                )));
            }
            // Fail on the name here rather than three test points into the sweep.
            let mut probe = crate::core::StrategyParams::default();
            probe
                .set_knob(name, *min_value)
                .map_err(|e| SimulationError::InvalidConfig(format!("Robustness range for '{}': {}", name, e)))?;
        }

        self.robustness_params = params;
        self.is_configured = true;

        info!("Validation pipeline configured successfully");
        Ok(())
    }

    /// Run robustness tests: replay the same data with one knob moved at a time.
    ///
    /// Each report carries the configuration that produced it, so the sweep's output
    /// can be read as "which value of which knob did what", which is the only thing a
    /// robustness run is for.
    pub async fn run_robustness_tests(
        &self,
        base_config: &SimulationConfig,
    ) -> Result<Vec<BacktestReport>, SimulationError> {
        if !self.is_configured {
            return Err(SimulationError::InvalidConfig(
                "Validation pipeline not configured".to_string(),
            ));
        }

        info!("Running robustness tests...");

        let events = self.load_events(base_config)?;
        let engine = BacktestingEngine::new();
        let mut reports = Vec::new();

        // For each parameter range, test variations
        for (param_name, min_val, max_val) in &self.robustness_params.parameter_ranges {
            info!("Testing robustness for parameter: {}", param_name);

            let step = (max_val - min_val) / self.robustness_params.test_points as f64;

            for i in 0..self.robustness_params.test_points {
                let test_value = min_val + (i as f64 * step);
                debug!(
                    "Test point {}/{} for '{}': {:.6}",
                    i + 1,
                    self.robustness_params.test_points,
                    param_name,
                    test_value
                );

                let mut test_config = base_config.clone();
                test_config.strategy.set_knob(param_name, test_value)?;

                let report = engine.run_simulation(&test_config, &events).await?;
                info!(
                    "'{}' = {:.6} -> return {:+.2}%, {} fill(s), drawdown {:.2}%",
                    param_name,
                    test_value,
                    report.results.total_return * 100.0,
                    report.results.total_trades,
                    report.results.max_drawdown * 100.0
                );
                reports.push(report);
            }
        }

        info!("Robustness tests completed with {} test cases", reports.len());
        Ok(reports)
    }

    /// Run overfitting detection tests.
    ///
    /// A tuned parameter shows up as a spike rather than a plateau: move each swept
    /// knob a little (`OVERFITTING_TEST_RANGE` of its range) and a fitted value
    /// collapses while a real edge merely drifts.
    ///
    /// `Ok(true)` means the neighbours hold up, i.e. *no* overfitting detected - the
    /// sense the original `Ok(!is_overfitted)` returned, kept so callers do not silently
    /// flip meaning.
    pub async fn run_overfitting_detection(
        &self,
        optimal_config: &SimulationConfig,
    ) -> Result<bool, SimulationError> {
        if !self.is_configured {
            return Err(SimulationError::InvalidConfig(
                "Validation pipeline not configured".to_string(),
            ));
        }

        info!("Running overfitting detection tests...");

        let events = self.load_events(optimal_config)?;
        let engine = BacktestingEngine::new();
        let optimal = engine.run_simulation(optimal_config, &events).await?;

        // "Does a small move destroy the result?" has no answer for a configuration that
        // does not make money: there is nothing to destroy. Saying "not overfitted" here
        // would be the same kind of false comfort the mock used to hand out.
        if optimal.results.total_return <= 0.0 {
            return Err(SimulationError::RobustnessTestFailed(format!(
                "Overfitting detection needs a profitable reference run; the config under test returned {:.2}%",
                optimal.results.total_return * 100.0
            )));
        }

        let mut worst: Option<f64> = None;
        let mut neighbours = 0usize;

        for (param_name, min_val, max_val) in &self.robustness_params.parameter_ranges {
            let centre = optimal_config.strategy.knob(param_name).ok_or_else(|| {
                SimulationError::InvalidConfig(format!(
                    "The simulation exposes no strategy parameter named '{}'",
                    param_name
                ))
            })?;

            // A nudge that is 5% of the swept range, or of the value itself where the
            // range has collapsed to a point.
            let nudge = ((max_val - min_val).abs() * OVERFITTING_TEST_RANGE)
                .max(centre.abs() * OVERFITTING_TEST_RANGE);

            for value in [centre - nudge, centre + nudge] {
                let mut config = optimal_config.clone();
                config.strategy.set_knob(param_name, value)?;

                let report = engine.run_simulation(&config, &events).await?;
                neighbours += 1;
                worst = Some(match worst {
                    Some(current) => current.min(report.results.total_return),
                    None => report.results.total_return,
                });
                debug!(
                    "Overfitting probe: '{}' {:.6} -> {:+.2}%",
                    param_name, value, report.results.total_return * 100.0
                );
            }
        }

        let worst = worst.ok_or_else(|| {
            SimulationError::InvalidConfig(
                "Overfitting detection swept no parameters; configure a range first".to_string(),
            )
        })?;

        let degradation = (optimal.results.total_return - worst) / optimal.results.total_return;
        let is_overfitted = degradation > MAX_ACCEPTABLE_PARAMETER_DEGRADATION;

        info!(
            "Overfitting check over {} perturbed run(s): best {:.2}%, worst {:.2}% ({:.1}% of the return lost)",
            neighbours,
            optimal.results.total_return * 100.0,
            worst * 100.0,
            degradation * 100.0
        );
        if is_overfitted {
            error!(
                "OVERFITTING DETECTED - a {:.4} move in a swept knob sheds {:.1}% of the return; \
                 this configuration should not be deployed",
                OVERFITTING_TEST_RANGE,
                degradation * 100.0
            );
        } else {
            info!("No overfitting detected - performance degrades gradually around the chosen parameters");
        }

        Ok(!is_overfitted)
    }

    /// Validate robustness results
    pub fn validate_robustness_results(&self, reports: &[BacktestReport]) -> Result<bool, SimulationError> {
        if reports.is_empty() {
            warn!("Robustness validation had no reports to judge; an empty sweep is not a pass");
            return Ok(false);
        }

        // Calculate performance statistics
        let avg_return: f64 = reports.iter().map(|r| r.results.total_return).sum::<f64>() / reports.len() as f64;
        let best_return = reports.iter().map(|r| r.results.total_return).fold(f64::NEG_INFINITY, f64::max);

        // Check if performance degrades gracefully
        let performance_ratio = if best_return > 0.0 {
            avg_return / best_return
        } else {
            // Nothing was profitable, so there is no performance to be robust about.
            // Dividing by zero here produced NaN, and `NaN < threshold` is false, which
            // read as a pass.
            0.0
        };

        let is_robust = performance_ratio >= self.robustness_params.min_acceptable_performance;

        if is_robust {
            info!("Strategy PASSED robustness validation (performance ratio: {:.3})", performance_ratio);
        } else {
            error!(
                "Strategy FAILED robustness validation (performance ratio: {:.3} < {:.3})",
                performance_ratio, self.robustness_params.min_acceptable_performance
            );
        }

        Ok(is_robust)
    }

    /// Check if configured
    pub fn is_configured(&self) -> bool {
        self.is_configured
    }

    /// The event sequence every test point replays.
    ///
    /// Read once per run set rather than per test point: the sweep is only a
    /// comparison if every variant saw identical data, and re-reading the lake would
    /// also multiply its I/O by the number of test points.
    fn load_events(&self, config: &SimulationConfig) -> Result<Vec<MarketEvent>, SimulationError> {
        let mut lake = DataLake::new();
        lake.initialize(&config.data_path)?;
        lake.collect_events(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{TimeZone, Utc};

    fn utc(millis: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_millis_opt(millis)
            .single()
            .expect("millisecond epoch inside chrono's range")
    }

    fn params(ranges: Vec<(&str, f64, f64)>, points: usize) -> RobustnessTestParams {
        RobustnessTestParams {
            parameter_ranges: ranges
                .into_iter()
                .map(|(name, min, max)| (name.to_string(), min, max))
                .collect(),
            test_points: points,
            min_acceptable_performance: crate::core::constants::MIN_ACCEPTABLE_ROBUSTNESS,
        }
    }

    #[test]
    fn a_sweep_of_a_knob_that_does_not_exist_is_refused() {
        let mut pipeline = ValidationPipeline::new();
        let error = pipeline
            .configure(params(vec![("leverage", 5.0, 20.0)], 5))
            .expect_err("SimulationConfig applies no 'leverage' strategy knob");
        assert!(
            error.to_string().contains("no strategy parameter named 'leverage'"),
            "{}",
            error
        );
    }

    #[test]
    fn an_unusable_range_or_point_count_is_refused() {
        let mut pipeline = ValidationPipeline::new();
        let empty: Vec<(&str, f64, f64)> = Vec::new();
        assert!(
            pipeline.configure(params(empty, 5)).is_err(),
            "a sweep with no ranges cannot produce a measurement"
        );

        assert!(
            pipeline
                .configure(params(vec![("obi_threshold", 0.5, 0.4)], 5))
                .is_err(),
            "an inverted interval is a typo, not a sweep"
        );

        assert!(
            pipeline
                .configure(params(vec![("obi_threshold", 0.4, 0.5)], 0))
                .is_err(),
            "zero test points divides by zero and then reports success"
        );
    }

    #[test]
    fn an_empty_sweep_is_not_reported_as_a_pass() {
        let mut pipeline = ValidationPipeline::new();
        pipeline.configure(params(vec![("obi_threshold", 0.4, 0.5)], 3)).unwrap();
        assert!(!pipeline.validate_robustness_results(&[]).unwrap());
    }

    #[test]
    fn unprofitable_variants_cannot_be_robust() {
        let mut pipeline = ValidationPipeline::new();
        pipeline.configure(params(vec![("obi_threshold", 0.4, 0.5)], 3)).unwrap();

        let report = BacktestReport {
            config: SimulationConfig::default(),
            results: crate::core::SimulationResult {
                total_return: 0.0,
                sharpe_ratio: 0.0,
                max_drawdown: 0.0,
                win_rate: 0.0,
                profit_factor: 0.0,
                total_trades: 0,
                avg_trade_duration: 0.0,
                slippage_impact: 0.0,
                execution_quality: 0.0,
            },
            equity_curve: vec![],
            trade_log: vec![],
            timestamp: chrono::Utc::now(),
        };

        assert!(!pipeline.validate_robustness_results(&[report]).unwrap());
    }

    #[test]
    fn every_swept_knob_round_trips_through_the_config() {
        let mut config = SimulationConfig::default();
        for &name in crate::core::StrategyParams::SWEPT_KNOBS {
            config.strategy.set_knob(name, 0.0).ok();
            // A zero is a legitimate probe only for the fractional knobs; the counted
            // ones are floored at one so a division never sees zero.
            let value = config.strategy.knob(name).expect("swept knobs are readable");
            let floored = matches!(name, "obi_levels" | "volatility_window");
            assert!(
                (value < 1e-12 && !floored) || (floored && value >= 1.0),
                "{} -> {}",
                name,
                value
            );
        }
    }

    #[tokio::test]
    async fn a_sweep_applies_the_knob_it_claims_to_test() {
        let directory = std::env::temp_dir().join(format!("aegis_pipeline_{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");

        // One book that clears any threshold in the swept range, plus the print that
        // fills the queued bid and the one that reaches the target.
        let lines = [
            r#"{"symbol":"BTCUSDT","timestamp":1700000001000,"bids":[[40000.0,8.0]],"asks":[[40001.0,0.5]]}"#,
            r#"{"symbol":"BTCUSDT","timestamp":1700000001100,"price":40000.0,"quantity":1.0,"side":"sell"}"#,
            r#"{"symbol":"BTCUSDT","timestamp":1700000002000,"price":40401.0,"quantity":1.0,"side":"buy"}"#,
            r#"{"symbol":"BTCUSDT","timestamp":1700000003000,"bids":[[40000.0,8.0]],"asks":[[40001.0,0.5]]}"#,
            r#"{"symbol":"BTCUSDT","timestamp":1700000003100,"price":40000.0,"quantity":1.0,"side":"sell"}"#,
            // Round two clears its target as well. Both rounds have to be profitable
            // here: the robustness gate below is only meaningful against a run that made
            // money, which is itself one of the things this test pins.
            r#"{"symbol":"BTCUSDT","timestamp":1700000004000,"price":40450.0,"quantity":1.0,"side":"buy"}"#,
        ]
        .join("\n");
        std::fs::write(directory.join("events.jsonl"), lines).expect("fixture written");

        let config = SimulationConfig {
            data_path: directory.to_str().expect("utf-8 temp path").to_string(),
            start_time: utc(1_700_000_000_000),
            end_time: utc(1_700_000_010_000),
            symbols: vec!["BTCUSDT".to_string()],
            ..SimulationConfig::default()
        };

        let mut pipeline = ValidationPipeline::new();
        pipeline
            .configure(params(vec![("obi_threshold", 0.3, 0.5)], 2))
            .expect("a swept knob with a range");

        let reports = pipeline
            .run_robustness_tests(&config)
            .await
            .expect("the sweep replays the lake");

        assert_eq!(reports.len(), 2, "test_points x ranges");
        assert_eq!(
            reports[0].results.total_trades, 4,
            "two entries and two exits have to be booked from this fixture, got {:?}",
            reports[0]
                .trade_log
                .iter()
                .map(|fill| (fill.timestamp, format!("{:?}", fill.action)))
                .collect::<Vec<_>>()
        );
        assert!(
            reports[0].config.strategy.obi_threshold < reports[1].config.strategy.obi_threshold,
            "each report must carry the value it was measured at, got {} and {}",
            reports[0].config.strategy.obi_threshold,
            reports[1].config.strategy.obi_threshold
        );
        // Real numbers, not the constants the mock used to hand back: 800 trades and a
        // 12% return cannot come out of a six-row fixture.
        assert!(
            reports[0].results.total_return > 0.0,
            "{:?}",
            reports[0].results
        );
        assert!(reports[0].results.execution_quality > 0.0);

        // The same events at both ends of the range give the same answer, so the
        // pipeline's own judgement is that the run is not knife-edge - which is exactly
        // what a comparison of identical data can support.
        let robust = pipeline.validate_robustness_results(&reports).expect("a ratio");
        assert!(robust);

        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test]
    async fn overfitting_detection_refuses_a_reference_run_that_loses() {
        let directory = std::env::temp_dir().join(format!("aegis_pipeline_flat_{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");
        // A book with no signal at all (balanced top of book), so no order is ever
        // submitted and the return is exactly zero.
        std::fs::write(
            directory.join("events.jsonl"),
            r#"{"symbol":"BTCUSDT","timestamp":1700000001000,"bids":[[40000.0,1.0]],"asks":[[40001.0,1.0]]}"#,
        )
        .expect("fixture written");

        let config = SimulationConfig {
            data_path: directory.to_str().expect("utf-8 temp path").to_string(),
            start_time: utc(1_700_000_000_000),
            end_time: utc(1_700_000_010_000),
            symbols: vec!["BTCUSDT".to_string()],
            ..SimulationConfig::default()
        };

        let mut pipeline = ValidationPipeline::new();
        pipeline
            .configure(params(vec![("obi_threshold", 0.3, 0.5)], 2))
            .unwrap();

        let error = pipeline
            .run_overfitting_detection(&config)
            .await
            .expect_err("a strategy that never traded cannot be judged for overfitting");
        assert!(matches!(error, SimulationError::RobustnessTestFailed(_)), "{:?}", error);

        std::fs::remove_dir_all(&directory).ok();
    }
}
