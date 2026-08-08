//! How well this particular Mac runs AI models on its own.
//!
//! Two independent numbers — how much memory there is, and how fast the chip
//! is — combined so that the weaker one dominates, because the weaker one is
//! what you actually feel. A fast chip with too little memory cannot run a
//! large model at all, and a huge amount of memory does not help a slow chip.
//!
//! Everything here is an estimate. It is meant to answer "what can I sensibly
//! run?", not to be a benchmark.

use crate::hardware::{ChipTier, HardwareProfile};

/// Share of total memory a model can realistically use. macOS, the browser and
/// everything else need the rest, and a Mac that swaps to disk stops being
/// usable long before it runs out of memory on paper.
const USABLE_MEMORY_SHARE: f32 = 0.65;

/// Memory score anchors: (total RAM in GB, score). Points between these are
/// interpolated; beyond the last one the score stops climbing.
const MEMORY_ANCHORS: [(u32, u8); 6] = [(0, 0), (8, 30), (16, 55), (32, 75), (48, 85), (64, 95)];

/// Penalty applied to sustained performance on the fastest laptop chips.
/// A desktop holds its top speed indefinitely; a laptop of the same chip slows
/// down once it warms up.
const LAPTOP_THERMAL_PENALTY: f32 = 10.0;

/// No Mac scores zero — every Mac can run something, even if only the smallest
/// models or a cloud service.
const SCORE_FLOOR: f32 = 5.0;

/// Plain-English note explaining the `(Q4)` in every tier label. Renderers must
/// show this wherever tier labels appear; the labels themselves are fixed
/// strings that reports and tests match on.
// Rendered by `report.rs` in Task 11 and the TUI in Task 13; remove this allow
// when the first renderer shows it.
#[allow(dead_code)]
pub const QUANTISATION_NOTE: &str = "\"Q4\" means the model has been compressed to roughly a quarter of its original size so it \
     fits in memory. It is the normal way to run models locally, and costs a little accuracy.";

/// How comfortably a model of a given size fits in this Mac's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Fit {
    /// Plenty of room to spare.
    Great,
    /// Fits with room for everyday work alongside it.
    Ok,
    /// Fits, but only just.
    Tight,
    /// Does not fit.
    No,
}

/// One model size class, judged against this Mac.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelTierFit {
    pub label: &'static str,
    pub fits: Fit,
    /// Rough speed range in words-per-second-ish units (tokens per second),
    /// or `None` when the model does not fit at all.
    pub est_tok_s: Option<(u16, u16)>,
    /// One plain sentence telling the reader what to do about it.
    pub advice: String,
}

/// This Mac's ability to run models locally.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceScore {
    pub overall: u8,
    pub memory: u8,
    pub compute: u8,
    pub tiers: Vec<ModelTierFit>,
}

/// One model size class and what it costs to run.
struct SizeClass {
    label: &'static str,
    /// Working-set memory in GB: the compressed weights *plus* the space the
    /// conversation and the runtime need. This is deliberately larger than the
    /// download size — a model whose file just fits will not actually run.
    footprint_gb: f32,
    /// Indicative speed by chip tier, in the order of [`TIER_ORDER`].
    tok_s: [(u16, u16); 5],
}

/// The chip tiers, in the order the speed tables are written.
const TIER_ORDER: [ChipTier; 5] = [
    ChipTier::Intel,
    ChipTier::Base,
    ChipTier::Pro,
    ChipTier::Max,
    ChipTier::Ultra,
];

/// The five size classes Windrose reports on.
///
/// Speeds are drawn from community llama.cpp and MLX benchmarks and are
/// indicative only — real numbers vary with the model, the settings and how
/// long the machine has been working.
const SIZE_CLASSES: [SizeClass; 5] = [
    SizeClass {
        label: "3B class (Q4)",
        footprint_gb: 2.5,
        tok_s: [(3, 6), (40, 70), (60, 100), (80, 130), (95, 150)],
    },
    SizeClass {
        label: "7B class (Q4)",
        footprint_gb: 6.0,
        tok_s: [(2, 4), (15, 30), (30, 55), (45, 75), (55, 90)],
    },
    SizeClass {
        label: "13B class (Q4)",
        footprint_gb: 10.0,
        tok_s: [(1, 2), (9, 18), (18, 32), (28, 45), (35, 55)],
    },
    SizeClass {
        label: "30B class (Q4)",
        footprint_gb: 22.0,
        tok_s: [(1, 1), (4, 9), (9, 16), (14, 24), (18, 30)],
    },
    SizeClass {
        label: "70B class (Q4)",
        footprint_gb: 42.0,
        tok_s: [(1, 1), (2, 4), (4, 8), (7, 12), (9, 16)],
    },
];

/// Rate this Mac's ability to run AI models on its own.
// Consumed by `gather()` in Task 11; remove this allow when that lands.
#[allow(dead_code)]
pub fn score(hw: &HardwareProfile) -> DeviceScore {
    let memory = memory_score(hw.ram_gb);
    let compute = compute_score(hw);

    DeviceScore {
        overall: overall_score(memory, compute, hw),
        memory,
        compute,
        tiers: tiers(hw),
    }
}

/// Memory score, interpolated between [`MEMORY_ANCHORS`].
fn memory_score(ram_gb: u32) -> u8 {
    let last = MEMORY_ANCHORS[MEMORY_ANCHORS.len() - 1];
    if ram_gb >= last.0 {
        return last.1;
    }

    MEMORY_ANCHORS
        .windows(2)
        .find(|pair| ram_gb < pair[1].0)
        .map(|pair| {
            let ((low_gb, low), (high_gb, high)) = (pair[0], pair[1]);
            let span = (high_gb - low_gb) as f32;
            let position = (ram_gb - low_gb) as f32 / span;
            (low as f32 + position * (high - low) as f32).round() as u8
        })
        .unwrap_or(last.1)
}

/// Compute score from the chip tier, with a small bonus for a large GPU.
fn compute_score(hw: &HardwareProfile) -> u8 {
    let base: u8 = match hw.chip_tier {
        ChipTier::Intel => 10,
        ChipTier::Base => 55,
        ChipTier::Pro => 70,
        ChipTier::Max => 85,
        ChipTier::Ultra => 95,
    };
    let gpu_bonus = if hw.gpu_cores.is_some_and(|cores| cores >= 30) {
        5
    } else {
        0
    };
    base.saturating_add(gpu_bonus).min(100)
}

/// Combine the two scores so the weaker one dominates, then apply the laptop
/// thermal penalty.
fn overall_score(memory: u8, compute: u8, hw: &HardwareProfile) -> u8 {
    let weaker = memory.min(compute) as f32;
    let stronger = memory.max(compute) as f32;
    let mut overall = weaker * 0.6 + stronger * 0.4;

    // Only the fastest chips are fast enough for heat to be the limit.
    if hw.is_laptop && matches!(hw.chip_tier, ChipTier::Max | ChipTier::Ultra) {
        overall -= LAPTOP_THERMAL_PENALTY;
    }

    overall.max(SCORE_FLOOR).round() as u8
}

/// Judge every size class against this Mac.
fn tiers(hw: &HardwareProfile) -> Vec<ModelTierFit> {
    let usable = hw.ram_gb as f32 * USABLE_MEMORY_SHARE;
    let tier_index = TIER_ORDER
        .iter()
        .position(|tier| *tier == hw.chip_tier)
        .unwrap_or(0);

    SIZE_CLASSES
        .iter()
        .map(|class| {
            let fits = fit_for(usable / class.footprint_gb);
            ModelTierFit {
                label: class.label,
                fits,
                // A speed estimate for something that cannot run would be
                // noise; the reader needs the memory answer, not a number.
                est_tok_s: (fits != Fit::No).then(|| class.tok_s[tier_index]),
                advice: advice_for(fits),
            }
        })
        .collect()
}

/// How much headroom counts as comfortable.
fn fit_for(headroom: f32) -> Fit {
    match headroom {
        h if h >= 1.5 => Fit::Great,
        h if h >= 1.2 => Fit::Ok,
        h if h >= 1.0 => Fit::Tight,
        _ => Fit::No,
    }
}

/// One plain sentence per outcome. No jargon, and always actionable.
fn advice_for(fits: Fit) -> String {
    match fits {
        Fit::Great => "Runs comfortably — a good everyday choice",
        Fit::Ok => "Runs well, with room left for your other apps",
        Fit::Tight => "Only just fits — close other apps first, and expect it to slow down",
        Fit::No => "Won't fit in memory — use a cloud provider for this size",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a machine to score. Named arguments beat positional booleans:
    /// `hw(Pro, 48, laptop)` says what it means at the call site.
    fn hw(chip_tier: ChipTier, ram_gb: u32, is_laptop: bool) -> HardwareProfile {
        HardwareProfile {
            chip_name: "test chip".to_string(),
            chip_tier,
            ram_gb,
            gpu_cores: None,
            macos_major: 26,
            is_apple_silicon: chip_tier != ChipTier::Intel,
            is_laptop,
        }
    }

    fn tier<'a>(s: &'a DeviceScore, label: &str) -> &'a ModelTierFit {
        s.tiers
            .iter()
            .find(|t| t.label == label)
            .unwrap_or_else(|| panic!("no tier labelled {label}"))
    }

    #[test]
    fn m4_pro_48gb_scores_high_and_fits_32b() {
        let s = score(&hw(ChipTier::Pro, 48, true));

        assert!(s.overall >= 75, "overall was {}", s.overall);
        assert_eq!(tier(&s, "30B class (Q4)").fits, Fit::Ok);
        assert_eq!(tier(&s, "70B class (Q4)").fits, Fit::No);
    }

    #[test]
    fn intel_8gb_scores_low_with_cloud_advice() {
        let s = score(&hw(ChipTier::Intel, 8, false));

        assert!(s.overall <= 25, "overall was {}", s.overall);
        assert!(
            s.tiers
                .iter()
                .all(|t| t.fits == Fit::No || t.label.contains("3B")),
            "only the smallest models should fit on this machine"
        );
        assert!(
            tier(&s, "70B class (Q4)")
                .advice
                .contains("use a cloud provider")
        );
    }

    /// The bottleneck is what you feel. A very fast chip starved of memory
    /// must not score as though the memory were fine.
    #[test]
    fn the_weaker_half_dominates_the_overall_score() {
        let starved = score(&hw(ChipTier::Ultra, 8, false));
        let balanced = score(&hw(ChipTier::Base, 32, false));

        assert!(
            starved.overall < balanced.overall,
            "starved {} should score below balanced {}",
            starved.overall,
            balanced.overall
        );
        // Closer to its weak half (30) than to its strong half (95).
        assert!(starved.overall < 60);
    }

    /// The reason `is_laptop` exists: the same chip sustains less in a laptop.
    #[test]
    fn fast_laptops_take_a_thermal_penalty() {
        for chip in [ChipTier::Max, ChipTier::Ultra] {
            let laptop = score(&hw(chip, 64, true)).overall;
            let desktop = score(&hw(chip, 64, false)).overall;
            assert_eq!(
                desktop - laptop,
                10,
                "{chip:?} should lose exactly the thermal penalty"
            );
        }
    }

    /// Slower chips never reach the speeds where heat becomes the limit.
    #[test]
    fn ordinary_laptops_are_not_penalised() {
        for chip in [ChipTier::Base, ChipTier::Pro, ChipTier::Intel] {
            assert_eq!(
                score(&hw(chip, 32, true)).overall,
                score(&hw(chip, 32, false)).overall,
                "{chip:?} should not be penalised for being a laptop"
            );
        }
    }

    #[test]
    fn no_mac_scores_below_the_floor() {
        let s = score(&hw(ChipTier::Intel, 4, true));

        assert!(s.overall >= 5, "overall was {}", s.overall);
    }

    #[test]
    fn memory_score_follows_its_anchors() {
        for (ram, expected) in [(8, 30), (16, 55), (32, 75), (48, 85), (64, 95)] {
            assert_eq!(memory_score(ram), expected, "{ram} GB");
        }
        // Beyond the top anchor the score stops climbing.
        assert_eq!(memory_score(128), 95);
        // And between anchors it interpolates rather than stepping.
        let between = memory_score(24);
        assert!(
            between > 55 && between < 75,
            "24 GB scored {between}, expected between the 16 and 32 GB anchors"
        );
    }

    #[test]
    fn a_large_gpu_earns_a_small_bonus() {
        let mut big = hw(ChipTier::Pro, 32, false);
        big.gpu_cores = Some(40);
        let mut small = hw(ChipTier::Pro, 32, false);
        small.gpu_cores = Some(16);

        assert_eq!(compute_score(&big) - compute_score(&small), 5);
    }

    /// A speed estimate for a model that cannot load would be meaningless.
    #[test]
    fn models_that_do_not_fit_carry_no_speed_estimate() {
        let s = score(&hw(ChipTier::Base, 16, true));

        for t in &s.tiers {
            assert_eq!(
                t.est_tok_s.is_none(),
                t.fits == Fit::No,
                "{}: estimate and fit disagree",
                t.label
            );
        }
    }

    /// Faster chips must never be estimated as slower ones on the same model.
    #[test]
    fn speed_estimates_rise_with_chip_tier() {
        let ram = 64;
        let speeds: Vec<u16> = TIER_ORDER
            .iter()
            .map(|chip| {
                tier(&score(&hw(*chip, ram, false)), "7B class (Q4)")
                    .est_tok_s
                    .expect("7B fits in 64 GB on every chip")
                    .0
            })
            .collect();

        assert!(
            speeds.windows(2).all(|pair| pair[0] < pair[1]),
            "speeds should increase with tier: {speeds:?}"
        );
    }

    /// The plain-language rule: every tier tells the reader what to do.
    #[test]
    fn every_tier_carries_plain_english_advice() {
        let s = score(&hw(ChipTier::Pro, 32, true));

        for t in &s.tiers {
            assert!(t.advice.len() > 20, "{}: {}", t.label, t.advice);
            assert!(
                !t.advice.contains("Q4") && !t.advice.contains("quantis"),
                "{}: advice should not lean on jargon",
                t.label
            );
        }
        // The jargon in the labels themselves is explained once, centrally.
        assert!(QUANTISATION_NOTE.contains("compressed"));
    }

    #[test]
    fn every_score_stays_in_range() {
        for chip in TIER_ORDER {
            for ram in [4, 8, 16, 24, 32, 48, 64, 128, 512] {
                for laptop in [true, false] {
                    let s = score(&hw(chip, ram, laptop));
                    assert!(s.overall <= 100 && s.overall >= 5, "{chip:?} {ram}GB");
                    assert!(s.memory <= 100 && s.compute <= 100);
                    assert_eq!(s.tiers.len(), SIZE_CLASSES.len());
                }
            }
        }
    }

    /// More memory must never make a model fit *less* well.
    #[test]
    fn fit_never_gets_worse_as_memory_grows() {
        let rank = |f: Fit| match f {
            Fit::No => 0,
            Fit::Tight => 1,
            Fit::Ok => 2,
            Fit::Great => 3,
        };

        for label in SIZE_CLASSES.map(|c| c.label) {
            let mut previous = 0;
            for ram in [4, 8, 16, 24, 32, 48, 64, 128] {
                let current = rank(tier(&score(&hw(ChipTier::Pro, ram, false)), label).fits);
                assert!(current >= previous, "{label} got worse at {ram} GB");
                previous = current;
            }
        }
    }
}
