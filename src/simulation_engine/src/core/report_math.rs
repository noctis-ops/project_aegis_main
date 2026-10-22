//! Sums whose result is *reported*, rather than merely added up.
//!
//! This layer reports money. A figure that reaches a console line or a `--report` file has to
//! be readable without being re-derived from an implementation detail of the standard library.

/// Sum an `f64` series with exactly one sign for zero.
///
/// `Iterator::sum` for floats is defined in the standard library as
/// `iter.fold(-0.0, |a, b| a + b)` - a float sum starts at *negative* zero
/// (`float_sum_product!`, `library/core/src/iter/traits/accum.rs`). So a sum over nothing,
/// or over nothing but zeros, comes back as `-0.0`.
///
/// No gate can catch that: `-0.0 == 0.0` is true and `-0.0 < 0.0` is false, which is why
/// `fees_paid` measured correctly while printing `fees -0.0000 USDT` - a line that reads like
/// the venue paid the strategy back, in the exact run an operator is trying to triage.
///
/// For any series holding a non-zero value this returns the same bits `sum()` returns: both
/// are a plain left fold of the same numbers, so no magnitude, metric or gate changes. The
/// only value affected is the sign of a total that is zero - and a cost, a return or an
/// exposure has exactly one of those to report.
pub fn sum_reported(values: impl IntoIterator<Item = f64>) -> f64 {
    values.into_iter().fold(0.0, |total, value| total + value)
}
