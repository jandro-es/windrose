//! The probe registry.
//!
//! Each probe answers one question — "is this AI option here, and is it
//! usable?" — and returns a [`Detection`]. Probes are independent, so they run
//! concurrently, but results always come back in registry order: reports,
//! snapshot tests and the TUI table all index by position.
//!
//! Adding a probe: implement [`Probe`] in its own file under `probes/`, write
//! the three-state tests first (ready / half-configured / absent), then add it
//! to [`registry`].

mod apple;
mod cloud;
mod llamacpp;
mod lmstudio;
mod mlx;
mod ollama;

use crate::model::Detection;
use crate::sys::SysCtx;

/// One detectable AI option.
///
/// `Sync` is required because [`run_all`] hands `&dyn Probe` to a worker
/// thread. Probes hold no mutable state, so this costs nothing in practice.
pub trait Probe: Sync {
    /// Stable machine identifier, matching [`Detection::id`].
    fn id(&self) -> &'static str;

    /// Look for this option. Must never panic and never block indefinitely —
    /// all OS access goes through `sys`, which enforces its own timeouts.
    fn detect(&self, sys: &dyn SysCtx) -> Detection;
}

/// The first version-shaped token in a tool's `--version` output.
///
/// Every tool spells this differently — `ollama version is 0.31.0`,
/// `2.1.226 (Claude Code)`, `codex-cli 0.147.0` — but all of them put a dotted
/// number somewhere in the output, and the first one is always the tool's own.
fn first_version_token(raw: &str) -> Option<String> {
    raw.split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()) && token.contains('.'))
        .map(|token| {
            token
                .trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_string()
        })
}

/// Every probe Windrose knows about, in the order results are presented.
///
/// Order is grouped by category — the things most people already have first,
/// then the speed-focused engines — because reports render this list top to
/// bottom without regrouping.
// Further probes are registered here as Tasks 7-8 add them.
pub fn registry() -> Vec<Box<dyn Probe>> {
    let mut probes: Vec<Box<dyn Probe>> = vec![
        Box::new(ollama::OllamaProbe),
        Box::new(lmstudio::LmStudioProbe),
        Box::new(llamacpp::LlamaCppProbe),
        Box::new(mlx::MlxProbe),
        Box::new(apple::AppleFmProbe),
    ];
    probes.extend(
        cloud::probes()
            .into_iter()
            .map(|probe| Box::new(probe) as Box<dyn Probe>),
    );
    probes
}

/// Run every registered probe against this machine.
pub fn run_all(sys: &(dyn SysCtx + Sync)) -> Vec<Detection> {
    run_probes(&registry(), sys)
}

/// The testable core of [`run_all`], taking the probe list as an argument so
/// tests can inject fakes instead of the real registry.
///
/// Every probe gets its own thread: they are dominated by subprocess spawns and
/// localhost requests, so running them one after another would make a scan take
/// as long as the sum of its slowest parts.
fn run_probes(probes: &[Box<dyn Probe>], sys: &(dyn SysCtx + Sync)) -> Vec<Detection> {
    std::thread::scope(|scope| {
        // Spawn first, then join in order — collecting the handles up front is
        // what makes this concurrent rather than a sequential spawn/join loop.
        let handles: Vec<_> = probes
            .iter()
            .map(|probe| scope.spawn(move || probe.detect(sys)))
            .collect();

        probes
            .iter()
            .zip(handles)
            .map(|(probe, handle)| {
                handle
                    .join()
                    .unwrap_or_else(|_| panic!("probe '{}' panicked", probe.id()))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Availability, Category, Detection};
    use crate::sys::testing::MockSys;
    use std::time::{Duration, Instant};

    struct FakeProbe {
        id: &'static str,
        /// Lets a test control the order in which probes *finish*, which is
        /// independent of the order they were spawned in.
        delay: Duration,
    }

    impl Probe for FakeProbe {
        fn id(&self) -> &'static str {
            self.id
        }
        fn detect(&self, _sys: &dyn SysCtx) -> Detection {
            std::thread::sleep(self.delay);
            Detection {
                id: self.id,
                name: self.id,
                category: Category::LocalRuntime,
                availability: Availability::Ready,
                version: None,
                details: Vec::new(),
                friendly: String::new(),
            }
        }
    }

    fn fakes(ids: &[&'static str]) -> Vec<Box<dyn Probe>> {
        ids.iter()
            .map(|id| {
                Box::new(FakeProbe {
                    id,
                    delay: Duration::ZERO,
                }) as Box<dyn Probe>
            })
            .collect()
    }

    /// Each probe sleeps longer than the one after it, so completion order is
    /// the exact reverse of registry order. An implementation that collected
    /// results as threads finished would return them backwards here.
    ///
    /// The elapsed-time assertion also pins down that probes really do run
    /// concurrently: run one after another this would take at least the sum of
    /// the delays, not the longest of them.
    #[test]
    fn preserves_order_when_probes_finish_in_reverse() {
        const STEP: Duration = Duration::from_millis(60);
        let probes: Vec<Box<dyn Probe>> = ["first", "second", "third", "fourth"]
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                Box::new(FakeProbe {
                    id,
                    delay: STEP * (4 - i as u32),
                }) as Box<dyn Probe>
            })
            .collect();

        let started = Instant::now();
        let found = run_probes(&probes, &MockSys::new());
        let elapsed = started.elapsed();

        assert_eq!(
            found.iter().map(|d| d.id).collect::<Vec<_>>(),
            ["first", "second", "third", "fourth"]
        );
        // Sequential would be 4+3+2+1 = 10 steps; concurrent is bounded by the
        // slowest probe at 4. The margin is deliberately wide for slow runners.
        assert!(
            elapsed < STEP * 8,
            "probes did not run concurrently: took {elapsed:?}"
        );
    }

    /// Reports, snapshots and the TUI table all index by position, so a scan
    /// must return detections in registry order however the threads finish.
    #[test]
    fn run_probes_preserves_registry_order() {
        let probes = fakes(&["first", "second", "third", "fourth"]);
        let found = run_probes(&probes, &MockSys::new());

        assert_eq!(
            found.iter().map(|d| d.id).collect::<Vec<_>>(),
            ["first", "second", "third", "fourth"]
        );
    }

    #[test]
    fn run_probes_runs_every_probe() {
        let probes = fakes(&["a", "b"]);
        let found = run_probes(&probes, &MockSys::new());

        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|d| d.availability == Availability::Ready));
    }

    #[test]
    fn run_probes_on_empty_registry_is_empty() {
        assert!(run_probes(&[], &MockSys::new()).is_empty());
    }

    /// Guards the registry as probes are added in Tasks 5-8: two probes
    /// sharing an id would make doctor checks and detail lookups ambiguous.
    #[test]
    fn registry_ids_are_unique() {
        let probes = registry();
        let mut ids: Vec<_> = probes.iter().map(|p| p.id()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate probe id in registry");
    }
}
