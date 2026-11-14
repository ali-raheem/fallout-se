//! Process-local identities. They are never serialized or reused, including after
//! an owner is dropped. No pointer address or caller-supplied value is authority.
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UniqueId(u64);

fn allocate(counter: &AtomicU64) -> Option<UniqueId> {
    let mut next = counter.load(Ordering::Relaxed);
    loop {
        let following = next.checked_add(1)?;
        match counter.compare_exchange_weak(next, following, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(UniqueId(next)),
            Err(current) => next = current,
        }
    }
}

impl UniqueId {
    /// Exhaustion panics before an owner is constructed. Never wrap or recycle.
    /// Relaxed ordering suffices: atomic allocation provides uniqueness, while
    /// ordinary ownership controls publication of the associated immutable data.
    pub(super) fn fresh() -> Self {
        allocate(&NEXT_ID).expect("process-local snapshot identity space exhausted")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_allocator_never_wraps_or_reuses_exhausted_values() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate(&counter), Some(UniqueId(u64::MAX - 1)));
        assert_eq!(allocate(&counter), None);
        assert_eq!(allocate(&counter), None);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn concurrent_allocations_are_distinct() {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| (0..64).map(|_| UniqueId::fresh().0).collect::<Vec<_>>())
            })
            .collect();
        let mut ids: Vec<_> = threads
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 512);
    }
}
