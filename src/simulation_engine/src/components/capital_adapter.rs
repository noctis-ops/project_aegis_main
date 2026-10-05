//! Capital Adapter for simulation testing

use crate::core::{CapitalAdaptationTestCase, BacktestReport};
use tracing::{debug, error, info};

/// Capital Adapter for testing different capital scenarios
pub struct CapitalAdapter;

impl CapitalAdapter {
    /// Create a new Capital Adapter
    pub fn new() -> Self {
        Self
    }
    
    /// Generate test cases for capital adaptation
    pub fn generate_test_cases(&self) -> Vec<CapitalAdaptationTestCase> {
        vec![
            // Small capital test case
            CapitalAdaptationTestCase {
                capital: crate::core::constants::TEST_CAPITAL_SMALL,
                leverage: 50.0,
                expected_min_notional_check: true,
                market_impact_threshold: 0.01, // 1% market impact
            },
            // Medium capital test case
            CapitalAdaptationTestCase {
                capital: crate::core::constants::TEST_CAPITAL_MEDIUM,
                leverage: 25.0,
                expected_min_notional_check: false,
                market_impact_threshold: 0.005, // 0.5% market impact
            },
            // Large capital test case
            CapitalAdaptationTestCase {
                capital: crate::core::constants::TEST_CAPITAL_LARGE,
                leverage: 10.0,
                expected_min_notional_check: false,
                market_impact_threshold: 0.001, // 0.1% market impact
            },
        ]
    }
    
    /// Validate test result against expectations
    pub fn validate_test_result(&self, report: &BacktestReport, test_case: &CapitalAdaptationTestCase) -> bool {
        info!("Validating capital adaptation test for capital=${:.2}", test_case.capital);
        
        // Check if strategy is profitable
        if report.results.total_return <= 0.0 {
            error!("Test failed: Strategy not profitable for capital=${:.2}", test_case.capital);
            return false;
        }
        
        // Check drawdown limits
        if report.results.max_drawdown > 0.2 { // 20% max drawdown
            error!("Test failed: Excessive drawdown ({:.2}%) for capital=${:.2}", 
                   report.results.max_drawdown * 100.0, test_case.capital);
            return false;
        }
        
        // Check execution quality
        if report.results.execution_quality < 0.8 { // 80% minimum execution quality
            error!("Test failed: Poor execution quality ({:.2}%) for capital=${:.2}", 
                   report.results.execution_quality * 100.0, test_case.capital);
            return false;
        }
        
        // For small capital, check MIN_NOTIONAL compliance
        if test_case.expected_min_notional_check {
            if !self.validate_min_notional_compliance(report) {
                error!("Test failed: MIN_NOTIONAL compliance issue for capital=${:.2}", test_case.capital);
                return false;
            }
        }
        
        // For large capital, check market impact tolerance
        if test_case.capital >= crate::core::constants::TEST_CAPITAL_LARGE {
            if report.results.slippage_impact > test_case.market_impact_threshold {
                error!("Test failed: Excessive market impact ({:.4}%) for capital=${:.2}", 
                       report.results.slippage_impact * 100.0, test_case.capital);
                return false;
            }
        }
        
        info!("Capital adaptation test PASSED for capital=${:.2}", test_case.capital);
        true
    }
    
    /// Validate MIN_NOTIONAL compliance against what the run actually booked.
    ///
    /// Layer 3's margin guard refuses orders below the venue minimum, so a fill under
    /// it is a fill the live path would never have received. Reading compliance off the
    /// strategy's profitability - what this used to do - grades a fabricated order size
    /// as a pass, which is the opposite of the check's purpose.
    fn validate_min_notional_compliance(&self, report: &BacktestReport) -> bool {
        let minimum = crate::core::constants::MIN_NOTIONAL_USDT;

        if report.trade_log.is_empty() {
            // No fills is not compliance. The execution-quality gate in
            // `validate_test_result` already fails such a run; this keeps the reason
            // from being reported as a lucky pass.
            error!("MIN_NOTIONAL check: the run booked no fills, so compliance cannot be claimed");
            return false;
        }

        let mut offenders = 0usize;
        let mut smallest = f64::INFINITY;

        for entry in &report.trade_log {
            let notional = entry.price * entry.quantity;
            if notional + 1e-9 < minimum {
                offenders += 1;
                if notional < smallest {
                    smallest = notional;
                }
            }
        }

        if offenders > 0 {
            error!(
                "MIN_NOTIONAL check: {} of {} fill(s) are below {:.2} USDT (smallest {:.6})",
                offenders,
                report.trade_log.len(),
                minimum,
                smallest
            );
            return false;
        }

        debug!(
            "MIN_NOTIONAL check: all {} fill(s) clear {:.2} USDT",
            report.trade_log.len(),
            minimum
        );
        true
    }
    
    /// Generate robustness test parameters
    pub fn generate_robustness_params(&self) -> crate::core::RobustnessTestParams {
        crate::core::RobustnessTestParams {
            parameter_ranges: vec![
                ("obi_threshold".to_string(), 0.4, 0.5),
                ("volatility_window".to_string(), 5.0, 15.0),
                ("risk_percentage".to_string(), 0.005, 0.02),
            ],
            test_points: 20,
            min_acceptable_performance: crate::core::constants::MIN_ACCEPTABLE_ROBUSTNESS,
        }
    }
    
    /// Validate robustness test results
    pub fn validate_robustness_results(&self, results: &[BacktestReport]) -> bool {
        if results.is_empty() {
            return false;
        }
        
        // Calculate average performance
        let avg_return: f64 = results.iter().map(|r| r.results.total_return).sum::<f64>() / results.len() as f64;
        let best_return = results.iter().map(|r| r.results.total_return).fold(f64::NEG_INFINITY, f64::max);
        
        // A ratio of returns is only meaningful against a run that made money: with
        // `best_return <= 0` the division yields NaN or a negative number, and
        // `NaN < 0.8` is false, which used to read as a pass for a strategy that never
        // profited at all. There is no such thing as a robust losing configuration.
        if best_return <= 0.0 {
            error!(
                "Robustness test failed: the best run returned {:.2}%, so there is no performance to be robust about",
                best_return * 100.0
            );
            return false;
        }

        // Check if performance degrades gracefully
        let performance_ratio = avg_return / best_return;
        
        if performance_ratio < crate::core::constants::MIN_ACCEPTABLE_ROBUSTNESS {
            error!("Robustness test failed: Performance ratio {:.3} < minimum {:.3}", 
                   performance_ratio, crate::core::constants::MIN_ACCEPTABLE_ROBUSTNESS);
            return false;
        }
        
        info!("Robustness test PASSED: Performance ratio {:.3}", performance_ratio);
        true
    }
}

/// A report carrying one fill per `(price, quantity)` pair, for the checks that read
/// the trade log rather than the summary numbers.
#[cfg(test)]
fn report_with_fills(fills: &[(f64, f64)]) -> crate::core::BacktestReport {
    crate::core::BacktestReport {
        config: crate::core::SimulationConfig::default(),
        results: crate::core::SimulationResult {
            total_return: if fills.is_empty() { 0.0 } else { 0.1 },
            sharpe_ratio: 0.0,
            max_drawdown: 0.0,
            win_rate: 0.0,
            profit_factor: 0.0,
            total_trades: fills.len(),
            avg_trade_duration: 0.0,
            slippage_impact: 0.0,
            execution_quality: 1.0,
        },
        equity_curve: Vec::new(),
        trade_log: fills
            .iter()
            .map(|(price, quantity)| crate::core::TradeLogEntry {
                timestamp: 1_700_000_000_000,
                symbol: "BTCUSDT".to_string(),
                action: crate::core::TradeAction::Entry,
                price: *price,
                quantity: *quantity,
                slippage: 0.0,
                fees: 0.0,
                pnl: 0.0,
            })
            .collect(),
        timestamp: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fill_below_the_venue_minimum_is_caught() {
        let adapter = CapitalAdapter::new();
        // 0.0001 BTC at 40000 is 4 USDT of notional: the live path would reject it.
        let report = report_with_fills(&[(40_000.0, 0.0001)]);
        assert!(!adapter.validate_min_notional_compliance(&report));
    }

    #[test]
    fn fills_at_or_above_the_minimum_pass() {
        let adapter = CapitalAdapter::new();
        let report = report_with_fills(&[(40_000.0, 0.000125), (40_000.0, 0.25)]);
        assert!(adapter.validate_min_notional_compliance(&report));
    }

    #[test]
    fn a_run_without_fills_claims_no_compliance() {
        let adapter = CapitalAdapter::new();
        assert!(
            !adapter.validate_min_notional_compliance(&report_with_fills(&[])),
            "profitability is not evidence that order sizes were placeable"
        );
    }

    #[test]
    fn capital_matrix_is_small_to_large_with_the_floor_check_on_the_small_case() {
        let adapter = CapitalAdapter::new();
        let cases = adapter.generate_test_cases();
        assert_eq!(cases.len(), 3);
        assert!(cases[0].expected_min_notional_check, "the small account is where the venue minimum binds");
        assert!(cases[0].capital < cases[1].capital && cases[1].capital < cases[2].capital);
    }

    #[test]
    fn unprofitable_runs_cannot_pass_the_robustness_gate() {
        let adapter = CapitalAdapter::new();

        // Every variant returned nothing: the ratio is 0/0 and a NaN comparison used
        // to fall through as a pass.
        let flat = vec![report_with_fills(&[]), report_with_fills(&[])];
        assert!(!adapter.validate_robustness_results(&flat));

        let mut losing = report_with_fills(&[(40_000.0, 0.25)]);
        losing.results.total_return = -0.10;
        let mut less_losing = losing.clone();
        less_losing.results.total_return = -0.02;
        assert!(!adapter.validate_robustness_results(&[losing, less_losing]));

        // Graceful degradation: the variants sit close to the best run.
        let mut base = report_with_fills(&[(40_000.0, 0.25)]);
        base.results.total_return = 0.10;
        let mut nearby = base.clone();
        nearby.results.total_return = 0.09;
        assert!(adapter.validate_robustness_results(&[base, nearby]));

        // A collapse away from the best run is exactly what the gate is for.
        let mut collapsed = report_with_fills(&[(40_000.0, 0.25)]);
        collapsed.results.total_return = 0.001;
        assert!(!adapter.validate_robustness_results(&[
            {
                let mut best = report_with_fills(&[(40_000.0, 0.25)]);
                best.results.total_return = 0.10;
                best
            },
            collapsed
        ]));
    }
}
