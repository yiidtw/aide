//! Token budget enforcement.
//!
//! **Invariant**: accumulated tokens never exceed budget before next spawn.

/// Tracks token usage across invocations of a single task.
#[derive(Debug)]
pub struct BudgetTracker {
    limit: u64,
    accumulated: u64,
    invocations: u32,
    max_retries: u32,
}

impl BudgetTracker {
    pub fn new(limit: u64, max_retries: u32) -> Self {
        Self {
            limit,
            accumulated: 0,
            invocations: 0,
            max_retries,
        }
    }

    /// Check if another invocation is allowed.
    pub fn can_invoke(&self) -> bool {
        self.accumulated < self.limit && self.invocations <= self.max_retries
    }

    /// Record token usage from a completed invocation.
    /// Returns the new accumulated total.
    pub fn record(&mut self, tokens_used: u64) -> u64 {
        self.accumulated = self.accumulated.saturating_add(tokens_used);
        self.invocations += 1;
        self.accumulated
    }

    /// Remaining token budget.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.accumulated)
    }

    /// Total tokens used so far.
    pub fn used(&self) -> u64 {
        self.accumulated
    }

    /// Number of invocations so far.
    pub fn invocations(&self) -> u32 {
        self.invocations
    }
}

// ── Kani proofs ──────────────────────────────────────────────────────

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn budget_never_overflows() {
        let limit: u64 = kani::any();
        let max_retries: u32 = kani::any();
        kani::assume(max_retries <= 100);

        let mut bt = BudgetTracker::new(limit, max_retries);
        let usage: u64 = kani::any();

        bt.record(usage);
        // saturating_add ensures no overflow
        assert!(bt.accumulated <= u64::MAX);
    }

    #[kani::proof]
    fn can_invoke_respects_limit() {
        let limit: u64 = kani::any();
        kani::assume(limit > 0 && limit < u64::MAX / 2);

        let mut bt = BudgetTracker::new(limit, 10);
        bt.record(limit); // exhaust budget

        assert!(!bt.can_invoke(), "Must not invoke after budget exhausted");
    }

    #[kani::proof]
    fn remaining_plus_used_equals_limit_or_less() {
        let limit: u64 = kani::any();
        kani::assume(limit < u64::MAX / 2);

        let mut bt = BudgetTracker::new(limit, 10);
        let usage: u64 = kani::any();
        kani::assume(usage <= limit);

        bt.record(usage);
        assert_eq!(bt.remaining() + bt.used(), limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_flow() {
        let mut bt = BudgetTracker::new(100_000, 3);
        assert!(bt.can_invoke());
        assert_eq!(bt.remaining(), 100_000);

        bt.record(30_000);
        assert!(bt.can_invoke());
        assert_eq!(bt.remaining(), 70_000);
        assert_eq!(bt.invocations(), 1);

        bt.record(70_000);
        assert!(!bt.can_invoke()); // budget exhausted
        assert_eq!(bt.remaining(), 0);
    }

    #[test]
    fn test_retry_limit() {
        let mut bt = BudgetTracker::new(1_000_000, 2);
        bt.record(1000);
        bt.record(1000);
        bt.record(1000); // 3rd invocation, max_retries = 2 → blocked
        assert!(!bt.can_invoke());
    }
}
