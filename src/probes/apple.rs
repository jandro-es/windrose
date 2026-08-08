//! Apple's built-in on-device model, shipped as part of macOS 26 and later.
//!
//! Nothing to install and nothing to run: it is either part of the operating
//! system on this Mac or it is not. Three things have to line up — a new enough
//! macOS, an Apple Silicon chip, and the framework actually present on disk.

use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

/// The first macOS release to ship the on-device model.
const FIRST_SUPPORTED_MAJOR: u32 = 26;

/// Present only when the operating system actually carries the model.
const FRAMEWORK: &str = "/System/Library/Frameworks/FoundationModels.framework";

/// Where macOS records whether the user turned Apple Intelligence on.
const OPT_IN_DOMAIN: &str = "com.apple.CloudSubscriptionFeatures.optIn";

/// Shown whenever we cannot say for certain that the feature is on. Pointing at
/// the exact settings pane beats a guess the user would have to verify anyway.
const OPT_IN_UNKNOWN: &str = "Unknown — check System Settings ▸ Apple Intelligence & Siri";

pub struct AppleFmProbe;

impl Probe for AppleFmProbe {
    fn id(&self) -> &'static str {
        "apple-fm"
    }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        let version = sys.run("sw_vers", &["-productVersion"]);
        let major = version.as_deref().and_then(major_version);
        let new_enough = major.is_some_and(|m| m >= FIRST_SUPPORTED_MAJOR);

        let apple_silicon = sys
            .run("sysctl", &["-n", "machdep.cpu.brand_string"])
            .is_some_and(|brand| brand.starts_with("Apple"));

        let framework_present = sys.path_exists(FRAMEWORK);

        let mut details = Vec::new();
        if new_enough {
            details.push(("Generation".to_string(), generation(major)));
            details.push(("Apple Intelligence".to_string(), opt_in_state(sys)));
        } else {
            details.push((
                "How to get it".to_string(),
                format!(
                    "Included free with macOS {FIRST_SUPPORTED_MAJOR} and later — this Mac runs \
                     macOS {}",
                    version.as_deref().unwrap_or("an older version")
                ),
            ));
        }

        let availability = match (new_enough, apple_silicon, framework_present) {
            (true, true, true) => Availability::Ready,
            // Apple ships the on-device model only for Apple Silicon; no
            // update to this Mac will change that.
            (true, false, _) => Availability::Partial("Requires Apple Silicon".to_string()),
            (true, true, false) => {
                Availability::Partial("macOS has not installed the model yet".to_string())
            }
            (false, _, _) => Availability::NotFound,
        };

        Detection {
            id: "apple-fm",
            name: "Apple Foundation Models",
            category: Category::ApplePlatform,
            availability,
            version,
            details,
            friendly: "Apple Foundation Models — the AI model Apple builds into macOS itself, \
                       free and running entirely on your Mac"
                .to_string(),
        }
    }
}

/// The leading number of `27.0`, `26.1.1` and so on.
fn major_version(raw: &str) -> Option<u32> {
    raw.split('.').next()?.trim().parse().ok()
}

/// What the user gets, in words rather than model specifications.
fn generation(major: Option<u32>) -> String {
    match major {
        Some(26) => "macOS 26's built-in model — small and fast, works without an internet \
                     connection"
            .to_string(),
        Some(m) => format!(
            "macOS {m}'s built-in model — an updated version, works without an internet connection"
        ),
        None => "Built into this version of macOS".to_string(),
    }
}

/// Best-effort read of the Apple Intelligence opt-in.
///
/// The domain holds an undocumented dictionary keyed by an opaque account id
/// (`{ 1301870110 = 1; }` on a machine with the feature on), not a plain flag.
/// A `1` in there is good evidence it is switched on; anything else — missing
/// domain, empty dictionary, a shape we do not recognise — is reported as
/// unknown rather than guessed at. Telling someone the feature is off when it
/// is on would send them to fix a problem they do not have.
fn opt_in_state(sys: &dyn SysCtx) -> String {
    let readable = sys.run("defaults", &["read", OPT_IN_DOMAIN]);
    match readable {
        Some(body) if body.contains("= 1;") => "Turned on".to_string(),
        _ => OPT_IN_UNKNOWN.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    /// The opt-in dictionary exactly as a real Mac prints it.
    const OPT_IN_ON: &str = "{\n    1301870110 = 1;\n}";

    fn mac(version: &str, chip: &str) -> MockSys {
        MockSys::new()
            .with_cmd("sw_vers -productVersion", version)
            .with_cmd("sysctl -n machdep.cpu.brand_string", chip)
    }

    #[test]
    fn macos27_apple_silicon_reports_ready() {
        let sys = mac("27.0", "Apple M4").with_path(FRAMEWORK);
        let d = AppleFmProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("27.0"));
        assert!(d.details.iter().any(|(k, _)| k == "Generation"));
    }

    #[test]
    fn macos26_and_27_describe_different_generations() {
        let older = AppleFmProbe.detect(&mac("26.2", "Apple M4").with_path(FRAMEWORK));
        let newer = AppleFmProbe.detect(&mac("27.0", "Apple M4").with_path(FRAMEWORK));

        let row = |d: &Detection| {
            d.details
                .iter()
                .find(|(k, _)| k == "Generation")
                .map(|(_, v)| v.clone())
                .expect("every supported macOS reports its generation")
        };
        assert!(row(&older).contains("26"));
        assert!(row(&newer).contains("27"));
        assert_ne!(row(&older), row(&newer));
    }

    /// An older macOS cannot have this at all, so the useful thing to say is
    /// what would change that.
    #[test]
    fn macos15_reports_not_found_with_upgrade_hint() {
        let sys = mac("15.6", "Apple M2");
        let d = AppleFmProbe.detect(&sys);

        assert_eq!(d.availability, Availability::NotFound);
        let hint = d
            .details
            .iter()
            .find(|(k, _)| k == "How to get it")
            .map(|(_, v)| v.clone())
            .expect("an unreachable option should say what would change that");
        assert!(hint.contains("26"));
        assert!(hint.contains("15.6"), "should name the version in use");
    }

    #[test]
    fn intel_mac_reports_partial() {
        let sys = mac("26.0", "Intel(R) Core(TM) i9").with_path(FRAMEWORK);
        let d = AppleFmProbe.detect(&sys);

        assert_eq!(
            d.availability,
            Availability::Partial("Requires Apple Silicon".to_string())
        );
    }

    /// A new enough macOS on the right chip, but the model itself has not
    /// landed on disk — half configured rather than absent.
    #[test]
    fn missing_framework_is_partial() {
        let d = AppleFmProbe.detect(&mac("26.0", "Apple M4"));

        assert!(matches!(d.availability, Availability::Partial(_)));
    }

    /// The opt-in domain holds an opaque dictionary, not a boolean.
    #[test]
    fn reads_the_real_opt_in_dictionary_shape() {
        let sys = mac("27.0", "Apple M4").with_path(FRAMEWORK).with_cmd(
            "defaults read com.apple.CloudSubscriptionFeatures.optIn",
            OPT_IN_ON,
        );
        let d = AppleFmProbe.detect(&sys);

        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Apple Intelligence" && v == "Turned on")
        );
    }

    /// Never fail the probe on the settings read, and never claim the feature
    /// is off just because the dictionary could not be understood.
    #[test]
    fn unreadable_opt_in_says_unknown_without_failing() {
        let sys = mac("27.0", "Apple M4").with_path(FRAMEWORK);
        let d = AppleFmProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Apple Intelligence" && v.starts_with("Unknown"))
        );
    }

    #[test]
    fn unrecognised_opt_in_contents_are_not_read_as_off() {
        let sys = mac("27.0", "Apple M4").with_path(FRAMEWORK).with_cmd(
            "defaults read com.apple.CloudSubscriptionFeatures.optIn",
            "{ }",
        );
        let d = AppleFmProbe.detect(&sys);

        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Apple Intelligence" && v.starts_with("Unknown"))
        );
    }

    /// A Mac that answers nothing at all must not look like a supported one.
    #[test]
    fn machine_that_answers_nothing_is_not_found() {
        let d = AppleFmProbe.detect(&MockSys::new());

        assert_eq!(d.availability, Availability::NotFound);
        assert!(d.version.is_none());
    }

    #[test]
    fn carries_a_plain_english_explanation() {
        let d = AppleFmProbe.detect(&MockSys::new());
        assert!(d.friendly.starts_with("Apple Foundation Models — "));
        assert!(d.friendly.len() > 30);
    }
}
