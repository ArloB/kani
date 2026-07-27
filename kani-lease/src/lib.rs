//! Lease/drain coordination for a hot-swappable source backend, isolated in its
//! own leaf crate so its one piece of hand-rolled lock-free logic can be
//! model-checked with `loom` without dragging tokio's dependency graph (which
//! breaks under a global `--cfg loom`) into the model build.
//!
//! `draining` and the lease `count` are packed into a **single** atomic word, and
//! a lease is taken with `compare_exchange`. This is the key to correctness: with
//! two separate atomics the lease/drain paths form a StoreLoad pair that only a
//! `SeqCst` total order makes safe — and `loom` cannot verify that (it does not
//! fully model `SeqCst`, treating it as Acquire/Release, so a correct `SeqCst`
//! fix reports a false positive). A single-location CAS needs no cross-location
//! ordering: the atomic's own modification order is the total order, which `loom`
//! models precisely. The model below passes.
//!
//! Run the model with:
//!   RUSTFLAGS="--cfg loom" cargo test -p kani-lease

#[cfg(loom)]
use loom::sync::atomic::AtomicUsize;
#[cfg(not(loom))]
use std::sync::atomic::AtomicUsize;

use std::sync::atomic::Ordering;

/// Top bit = draining; the rest = the count of live leases.
const DRAINING: usize = 1 << (usize::BITS - 1);

/// Shared `{draining | lease_count}` for a hot-swappable backend.
#[derive(Debug)]
pub struct LeaseCoordinator {
    state: AtomicUsize,
}

impl Default for LeaseCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl LeaseCoordinator {
    pub fn new() -> Self {
        Self {
            state: AtomicUsize::new(0),
        }
    }

    /// Try to take a lease. Returns `true` on success (the caller must
    /// [`release`](Self::release) exactly once when the lease ends), or `false`
    /// if the source is draining. The increment only commits — atomically — while
    /// the draining bit is clear, so a lease and a drain can never both "win".
    pub fn try_acquire(&self) -> bool {
        let mut cur = self.state.load(Ordering::Acquire);
        loop {
            if cur & DRAINING != 0 {
                return false;
            }
            match self.state.compare_exchange_weak(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => cur = actual,
            }
        }
    }

    pub fn release(&self) {
        self.state.fetch_sub(1, Ordering::AcqRel);
    }

    /// Mark the source draining. New `try_acquire` calls will fail from here on.
    pub fn start_drain(&self) {
        self.state.fetch_or(DRAINING, Ordering::AcqRel);
    }

    /// Live lease count — the value a drain checks to decide it may swap.
    pub fn active(&self) -> usize {
        self.state.load(Ordering::Acquire) & !DRAINING
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::LeaseCoordinator;

    #[test]
    fn a_released_lease_returns_the_count_to_zero() {
        let c = LeaseCoordinator::new();
        assert!(c.try_acquire());
        assert_eq!(c.active(), 1);
        c.release();
        assert_eq!(c.active(), 0);
    }

    #[test]
    fn a_lease_is_refused_once_draining() {
        let c = LeaseCoordinator::new();
        c.start_drain();
        assert!(!c.try_acquire(), "no lease after draining begins");
        assert_eq!(c.active(), 0, "a refused lease leaves the count at zero");
    }
}

#[cfg(loom)]
mod loom_tests {
    use super::LeaseCoordinator;
    use loom::sync::Arc;

    // The invariant: if a lease is granted and still live, a concurrent drain must
    // observe it (never sees zero and swaps out a live lease). Fails on
    // Acquire/Release, passes on SeqCst.
    #[test]
    fn drain_never_misses_a_concurrent_lease() {
        loom::model(|| {
            let coord = Arc::new(LeaseCoordinator::new());
            let drainer = {
                let c = Arc::clone(&coord);
                loom::thread::spawn(move || {
                    c.start_drain();
                    c.active() // the count the drain sees at its decision point
                })
            };

            let leased = coord.try_acquire();
            let observed = drainer.join().unwrap();

            if leased {
                assert!(
                    observed >= 1,
                    "drain observed 0 leases while a lease was live"
                );
                coord.release();
            }
        });
    }
}
