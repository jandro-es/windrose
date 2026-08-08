//! What this Mac is made of.
//!
//! Everything here comes from four cheap, read-only commands routed through
//! [`SysCtx`], so the whole profile is reproducible in tests. A Mac that
//! declines to answer any given question degrades to a sensible default rather
//! than failing the scan.

use crate::sys::SysCtx;

/// Which member of the Apple Silicon family this chip belongs to.
///
/// The tier drives the compute half of the scoring engine: within a
/// generation, a Max has roughly twice the memory bandwidth of a Pro, which
/// matters far more for running AI models than raw CPU speed does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChipTier {
    Base,
    Pro,
    Max,
    Ultra,
    /// Pre-Apple-Silicon Intel Mac. Runs models, but slowly and without the
    /// unified-memory advantage.
    Intel,
}

/// A plain description of the machine Windrose is running on.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HardwareProfile {
    /// As reported by the CPU itself, e.g. "Apple M4 Pro".
    pub chip_name: String,
    pub chip_tier: ChipTier,
    /// Total physical memory, rounded to the nearest whole gigabyte.
    pub ram_gb: u32,
    /// GPU core count, when the system reports one. Intel Macs do not.
    pub gpu_cores: Option<u32>,
    /// Major macOS version, e.g. 27. Zero when it could not be read.
    pub macos_major: u32,
    pub is_apple_silicon: bool,
    /// Laptops throttle on sustained load, which the scoring engine accounts for.
    pub is_laptop: bool,
}

/// Build a [`HardwareProfile`] by asking the machine about itself.
pub fn profile(sys: &dyn SysCtx) -> HardwareProfile {
    let chip_name = sys
        .run("sysctl", &["-n", "machdep.cpu.brand_string"])
        .unwrap_or_else(|| "Unknown".to_string());

    // Apple's own chips are the only ones that report a brand starting "Apple".
    let is_apple_silicon = chip_name.starts_with("Apple");

    HardwareProfile {
        chip_tier: chip_tier(&chip_name, is_apple_silicon),
        ram_gb: ram_gb(sys),
        gpu_cores: gpu_cores(sys),
        macos_major: macos_major(sys),
        is_laptop: is_laptop(sys),
        chip_name,
        is_apple_silicon,
    }
}

fn chip_tier(chip_name: &str, is_apple_silicon: bool) -> ChipTier {
    if !is_apple_silicon {
        return ChipTier::Intel;
    }
    // Checked most specific first; a plain "Apple M4" carries no suffix.
    if chip_name.contains("Ultra") {
        ChipTier::Ultra
    } else if chip_name.contains("Max") {
        ChipTier::Max
    } else if chip_name.contains("Pro") {
        ChipTier::Pro
    } else {
        ChipTier::Base
    }
}

/// `hw.memsize` is in bytes and always a whole number of binary gigabytes;
/// rounding keeps a machine that reports a few megabytes short from showing
/// up as 15 GB.
fn ram_gb(sys: &dyn SysCtx) -> u32 {
    const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;
    sys.run("sysctl", &["-n", "hw.memsize"])
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(|bytes| (bytes as f64 / BYTES_PER_GB).round() as u32)
        .unwrap_or(0)
}

fn macos_major(sys: &dyn SysCtx) -> u32 {
    sys.run("sw_vers", &["-productVersion"])
        .and_then(|v| v.trim().split('.').next()?.parse().ok())
        .unwrap_or(0)
}

/// Apple Silicon reports its GPU core count in the displays profile. Intel
/// Macs omit the field entirely, and a malformed payload is treated the same
/// as a missing one.
fn gpu_cores(sys: &dyn SysCtx) -> Option<u32> {
    let raw = sys.run("system_profiler", &["SPDisplaysDataType", "-json"])?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("SPDisplaysDataType")?
        .as_array()?
        .iter()
        .find_map(|gpu| gpu.get("sppci_cores")?.as_str()?.trim().parse().ok())
}

/// Whether this is a laptop, which the scoring engine penalises slightly for
/// sustained load because laptops throttle where desktops do not.
///
/// A built-in battery is the reliable signal. The obvious test — "does the
/// model identifier contain Book?" — silently fails on current hardware:
/// Apple moved to generic identifiers, so a MacBook Pro now reports "Mac17,9".
/// The model check is kept only as a fallback for older Macs (MacBookPro16,1)
/// and for the case where `pmset` cannot be read.
fn is_laptop(sys: &dyn SysCtx) -> bool {
    if let Some(power) = sys.run("pmset", &["-g", "batt"]) {
        return power.contains("InternalBattery");
    }
    sys.run("sysctl", &["-n", "hw.model"])
        .is_some_and(|model| model.contains("Book"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    /// A Mac that answers every question we ask it.
    fn full_mock(brand: &str, memsize: &str, os: &str, model: &str, gpu_json: &str) -> MockSys {
        MockSys::new()
            .with_cmd("sysctl -n machdep.cpu.brand_string", brand)
            .with_cmd("sysctl -n hw.memsize", memsize)
            .with_cmd("sysctl -n hw.model", model)
            .with_cmd("sw_vers -productVersion", os)
            .with_cmd("system_profiler SPDisplaysDataType -json", gpu_json)
    }

    #[test]
    fn parses_m4_pro_mac() {
        let sys = MockSys::new()
            .with_cmd("sysctl -n machdep.cpu.brand_string", "Apple M4 Pro")
            .with_cmd("sysctl -n hw.memsize", "51539607552")
            .with_cmd("sw_vers -productVersion", "27.0")
            .with_cmd(
                "system_profiler SPDisplaysDataType -json",
                r#"{"SPDisplaysDataType":[{"sppci_cores":"20"}]}"#,
            );
        let hw = profile(&sys);
        assert_eq!(hw.chip_tier, ChipTier::Pro);
        assert_eq!(hw.ram_gb, 48);
        assert_eq!(hw.macos_major, 27);
        assert!(hw.is_apple_silicon);
        assert_eq!(hw.gpu_cores, Some(20));
        assert_eq!(hw.chip_name, "Apple M4 Pro");
    }

    #[test]
    fn parses_intel_mac() {
        let sys = full_mock(
            "Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz",
            "17179869184",
            "15.7.2",
            "MacBookPro16,1",
            r#"{"SPDisplaysDataType":[{"spdisplays_vendor":"Intel"}]}"#,
        );
        let hw = profile(&sys);
        assert_eq!(hw.chip_tier, ChipTier::Intel);
        assert_eq!(hw.gpu_cores, None);
        assert!(!hw.is_apple_silicon);
        assert_eq!(hw.ram_gb, 16);
        assert_eq!(hw.macos_major, 15);
    }

    #[test]
    fn parses_every_apple_silicon_tier() {
        for (brand, expected) in [
            ("Apple M1", ChipTier::Base),
            ("Apple M4", ChipTier::Base),
            ("Apple M3 Pro", ChipTier::Pro),
            ("Apple M2 Max", ChipTier::Max),
            ("Apple M1 Ultra", ChipTier::Ultra),
        ] {
            let sys = full_mock(brand, "17179869184", "26.0", "Mac16,1", "{}");
            assert_eq!(profile(&sys).chip_tier, expected, "brand: {brand}");
        }
    }

    #[test]
    fn detects_laptop_from_model() {
        let laptop = full_mock(
            "Apple M4 Pro",
            "17179869184",
            "26.0",
            "MacBookPro18,3",
            "{}",
        );
        assert!(profile(&laptop).is_laptop);

        let desktop = full_mock("Apple M4 Pro", "17179869184", "26.0", "Mac14,12", "{}");
        assert!(!profile(&desktop).is_laptop);
    }

    /// Apple moved to generic model identifiers: this MacBook Pro reports
    /// "Mac17,9", with no "Book" anywhere in it. Battery presence is what
    /// actually distinguishes a laptop on current hardware.
    #[test]
    fn detects_modern_laptop_without_book_in_model() {
        let sys = full_mock("Apple M5 Pro", "68719476736", "27.0", "Mac17,9", "{}").with_cmd(
            "pmset -g batt",
            "Now drawing from 'AC Power'\n -InternalBattery-0 (id=35586147)\t93%; charging; \
             (no estimate) present: true",
        );
        assert!(profile(&sys).is_laptop);
    }

    #[test]
    fn modern_desktop_has_no_battery() {
        let sys = full_mock("Apple M3 Ultra", "137438953472", "27.0", "Mac16,9", "{}")
            .with_cmd("pmset -g batt", "Now drawing from 'AC Power'");
        assert!(!profile(&sys).is_laptop);
    }

    #[test]
    fn rounds_ram_to_nearest_gb() {
        // 8 GiB exactly, and a machine reporting slightly under 16 GiB.
        let eight = full_mock("Apple M1", "8589934592", "26.0", "Mac16,1", "{}");
        assert_eq!(profile(&eight).ram_gb, 8);

        let almost_sixteen = full_mock("Apple M1", "17108176896", "26.0", "Mac16,1", "{}");
        assert_eq!(profile(&almost_sixteen).ram_gb, 16);
    }

    #[test]
    fn survives_a_machine_that_answers_nothing() {
        // Every probe command missing: the profiler must degrade, not panic.
        let hw = profile(&MockSys::new());
        assert_eq!(hw.ram_gb, 0);
        assert_eq!(hw.macos_major, 0);
        assert_eq!(hw.gpu_cores, None);
        assert!(!hw.is_apple_silicon);
        assert_eq!(hw.chip_tier, ChipTier::Intel);
    }

    #[test]
    fn ignores_malformed_gpu_json() {
        let sys = full_mock(
            "Apple M4",
            "17179869184",
            "26.0",
            "Mac16,1",
            "not json at all",
        );
        assert_eq!(profile(&sys).gpu_cores, None);
    }
}
