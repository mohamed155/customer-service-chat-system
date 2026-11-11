//! Identifier generation (ADR-013).
//!
//! All new primary keys are UUIDv7 (RFC 9562): a millisecond timestamp followed by random bits, so
//! inserts append to the right edge of the index instead of landing at random places. Application
//! code calls [`new_id`] and never `Uuid::new_v4()` for a key.
//!
//! `Uuid::new_v4()` stays for values that must not reveal when they were made or be guessable from
//! the clock: session token IDs and claim tokens. Each such call carries a `// uuid-v4-ok: <why>`
//! comment, and `server/tests/uuid_hygiene.rs` fails on any other.
//!
//! # Tests
//! [`install_generator`] replaces the generator for the rest of the process (or until the returned
//! guard drops), so a test can know the IDs in advance. It is process-wide: a test that installs one
//! must not share a process with tests that expect real IDs, which `cargo nextest` guarantees and
//! `cargo test` does not.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use uuid::Uuid;

/// Source of new identifiers.
pub trait IdGenerator: Send + Sync {
    fn next_id(&self) -> Uuid;
}

/// The production generator: `Uuid::now_v7()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemIdGenerator;

impl IdGenerator for SystemIdGenerator {
    fn next_id(&self) -> Uuid {
        Uuid::now_v7()
    }
}

/// A deterministic generator for tests: valid UUIDv7 values with a fixed timestamp and a counter,
/// so the same sequence comes out on every run and they still sort in creation order.
#[derive(Debug)]
pub struct SequentialIdGenerator {
    timestamp_ms: u64,
    counter: AtomicU64,
}

impl SequentialIdGenerator {
    /// Starts at counter 1 with the timestamp `2026-01-01T00:00:00Z`.
    pub fn new() -> Self {
        Self::with_timestamp_ms(1_767_225_600_000)
    }

    pub fn with_timestamp_ms(timestamp_ms: u64) -> Self {
        Self {
            timestamp_ms,
            counter: AtomicU64::new(1),
        }
    }
}

impl Default for SequentialIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdGenerator for SequentialIdGenerator {
    fn next_id(&self) -> Uuid {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut bytes = [0u8; 16];
        bytes[..6].copy_from_slice(&self.timestamp_ms.to_be_bytes()[2..]);
        bytes[6] = 0x70; // version 7
        bytes[8] = 0x80; // variant 10
        bytes[8..16].copy_from_slice(&(n | (0x80 << 56)).to_be_bytes());
        Uuid::from_bytes(bytes)
    }
}

/// The installed generator and the number of the installation that put it there.
static OVERRIDE: RwLock<Option<(u64, Arc<dyn IdGenerator>)>> = RwLock::new(None);
static INSTALLATIONS: AtomicU64 = AtomicU64::new(0);

/// A new identifier for a primary key: UUIDv7, from the installed generator if there is one.
pub fn new_id() -> Uuid {
    let installed = OVERRIDE.read().unwrap_or_else(|e| e.into_inner()).clone();
    match installed {
        Some((_, generator)) => generator.next_id(),
        None => Uuid::now_v7(),
    }
}

/// Use `generator` for [`new_id`] until the guard is dropped. See the module notes on scope.
///
/// Installing a second generator while a first is active replaces it; each guard only removes the
/// generator it installed, so dropping the older guard later leaves the newer one in place.
#[must_use = "the generator is removed when the guard is dropped"]
pub fn install_generator(generator: Arc<dyn IdGenerator>) -> GeneratorGuard {
    let installation = INSTALLATIONS.fetch_add(1, Ordering::Relaxed);
    *OVERRIDE.write().unwrap_or_else(|e| e.into_inner()) = Some((installation, generator));
    GeneratorGuard(installation)
}

/// Restores the system generator when dropped, unless a newer installation replaced its generator.
#[derive(Debug)]
pub struct GeneratorGuard(u64);

impl Drop for GeneratorGuard {
    fn drop(&mut self) {
        let mut installed = OVERRIDE.write().unwrap_or_else(|e| e.into_inner());
        if installed
            .as_ref()
            .is_some_and(|(owner, _)| *owner == self.0)
        {
            *installed = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn version(id: Uuid) -> usize {
        id.get_version_num()
    }

    #[test]
    #[serial(ids)]
    fn new_id_is_a_version_7_uuid_that_sorts_by_creation_time() {
        let a = new_id();
        std::thread::sleep(std::time::Duration::from_millis(3));
        let b = new_id();
        assert_eq!(version(a), 7);
        assert_eq!(version(b), 7);
        assert_eq!(a.get_variant(), uuid::Variant::RFC4122);
        assert!(a < b, "{a} should sort before {b}");
    }

    #[test]
    #[serial(ids)]
    fn a_installed_generator_is_used_until_its_guard_drops() {
        let guard = install_generator(Arc::new(SequentialIdGenerator::new()));
        let first = new_id();
        let second = new_id();
        assert_eq!(first.to_string(), "019b76da-a800-7000-8000-000000000001");
        assert_eq!(second.to_string(), "019b76da-a800-7000-8000-000000000002");
        drop(guard);

        let real = new_id();
        assert_ne!(real, new_id());
        assert!(real
            .get_timestamp()
            .is_some_and(|t| t.to_unix().0 > 1_767_225_600));
    }

    #[test]
    #[serial(ids)]
    fn an_older_guard_does_not_remove_a_newer_generator() {
        let first = Arc::new(SequentialIdGenerator::with_timestamp_ms(1_000_000_000_000));
        let second = Arc::new(SequentialIdGenerator::with_timestamp_ms(2_000_000_000_000));

        let older = install_generator(first);
        let newer = install_generator(second);
        drop(older);
        let id = new_id();
        assert_eq!(
            id.get_timestamp().unwrap().to_unix().0,
            2_000_000_000,
            "the newer generator is still installed"
        );

        drop(newer);
        let real = new_id();
        assert!(
            real.get_timestamp().unwrap().to_unix().0 < 1_900_000_000,
            "{real}"
        );
        assert_ne!(
            real.get_timestamp().unwrap().to_unix().0,
            2_000_000_000,
            "the system generator is back"
        );
    }

    #[test]
    fn sequential_ids_are_valid_v7_and_in_order() {
        let generator = SequentialIdGenerator::with_timestamp_ms(1_700_000_000_000);
        let ids: Vec<Uuid> = (0..5).map(|_| generator.next_id()).collect();
        for id in &ids {
            assert_eq!(version(*id), 7);
            assert_eq!(id.get_variant(), uuid::Variant::RFC4122);
        }
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        assert_eq!(
            ids[0].get_timestamp().unwrap().to_unix().0,
            1_700_000_000,
            "the fixed timestamp is the embedded one"
        );
    }
}
