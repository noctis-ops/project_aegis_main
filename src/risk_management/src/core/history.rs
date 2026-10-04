//! Bounded-history helpers shared by the rolling risk detectors.

/// Trim a rolling history buffer down to its last `max_len` samples.
///
/// Every detector in this layer (latency, rejection rate, funding, volatility,
/// MAE/MFE) appends one sample per tick and must keep its window bounded, or a
/// 24/7 risk engine grows until it is OOM-killed next to an open position.
///
/// The subtraction is saturating on purpose: writing `history.drain(..len - max)`
/// directly is both a borrow conflict (the range borrows `history` immutably while
/// `drain` needs it mutably) and an underflow when the buffer is still shorter than
/// the cap — a panic in debug, a wrapped huge range in release.
pub fn trim_to_last<T>(history: &mut Vec<T>, max_len: usize) {
    let excess = history.len().saturating_sub(max_len);
    if excess > 0 {
        history.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_samples() {
        let mut history: Vec<u32> = vec![1, 2, 3, 4, 5];
        trim_to_last(&mut history, 2);
        assert_eq!(history, vec![4, 5]);
    }

    #[test]
    fn short_and_empty_buffers_are_left_alone() {
        let mut history: Vec<u32> = vec![1];
        trim_to_last(&mut history, 100);
        assert_eq!(history, vec![1]);

        let mut empty: Vec<u32> = Vec::new();
        trim_to_last(&mut empty, 100);
        assert!(empty.is_empty());
    }
}
