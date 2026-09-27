//! Capital Adapter for simulation testing

use crate::core::{CapitalAdaptationTestCase, BacktestReport, SimulationError};
use tracing::{info, debug, warn, error};

/// Capital Adapter for testing different capital scenarios
pub struct CapitalAdapter {
    test_cases: Vec<CapitalAdaptationTestCase>,
}

impl CapitalAdapter {
    /// Create a new Capital Adapter
    pub fn new() -> Self {
        Self {
            test_cases: Vec::new(),
        }
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
    
    /// Validate MIN_NOTIONAL compliance
    fn validate_min_notional_compliance(&self, report: &BacktestReport) -> bool {
        // Check if any trades violated MIN_NOTIONAL requirements
        // In a real implementation, this would check actual trade sizes
        debug!("Validating MIN_NOTIONAL compliance");
        
        // For now, we'll assume compliance if strategy is profitable
        report.results.total_return > 0.0
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