//! llama.cpp — the engine most other local AI apps are built on.
//!
//! Unlike Ollama and LM Studio there is no background service to find: the
//! binaries are run on demand, so having them installed is the same as being
//! ready to use.

use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

/// The binaries to look for, in the order their version output is preferred.
const BINARIES: [&str; 2] = ["llama-server", "llama-cli"];

pub struct LlamaCppProbe;

impl Probe for LlamaCppProbe {
    fn id(&self) -> &'static str {
        "llamacpp"
    }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        let found = BINARIES
            .iter()
            .find_map(|bin| sys.run(bin, &["--version"]).map(|out| (*bin, out)));

        let brew = sys.run("brew", &["list", "llama.cpp", "--versions"]);

        let mut details = Vec::new();
        if let Some((bin, _)) = &found {
            details.push(("Command".to_string(), (*bin).to_string()));
        }
        if brew.is_some() {
            details.push(("Installed with Homebrew".to_string(), "yes".to_string()));
        }
        if !details.is_empty() && is_apple_silicon(sys) {
            details.push((
                "Metal acceleration".to_string(),
                // Plain-language rule: "Metal" means nothing to most readers.
                "yes — runs on your Mac's built-in graphics chip".to_string(),
            ));
        }

        // Prefer the binary's own version, then Homebrew's record of it. The
        // binaries print their version to stderr, which the OS layer discards,
        // so an empty-but-successful run means "installed, version unknown".
        let version = found
            .as_ref()
            .and_then(|(_, out)| parse_version(out))
            .or_else(|| brew.as_deref().and_then(parse_brew_version));

        // No daemon to start: if the binaries are here, they are usable.
        let availability = if found.is_some() || brew.is_some() {
            Availability::Ready
        } else {
            Availability::NotFound
        };

        Detection {
            id: "llamacpp",
            name: "llama.cpp",
            category: Category::OptimisedRuntime,
            availability,
            version,
            details,
            friendly: "llama.cpp — a fast engine that runs AI models straight on your Mac's chip"
                .to_string(),
        }
    }
}

/// Pull the build number out of `version: 4589 (a1b2c3d)`.
fn parse_version(raw: &str) -> Option<String> {
    raw.split_whitespace()
        .find(|token| token.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

/// Pull the version out of `llama.cpp 4589`, Homebrew's `--versions` format.
fn parse_brew_version(raw: &str) -> Option<String> {
    raw.split_whitespace().nth(1).map(str::to_string)
}

/// Metal is Apple's graphics layer; llama.cpp only uses it on Apple Silicon.
fn is_apple_silicon(sys: &dyn SysCtx) -> bool {
    sys.run("sysctl", &["-n", "machdep.cpu.brand_string"])
        .is_some_and(|brand| brand.starts_with("Apple"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    fn apple_silicon() -> MockSys {
        MockSys::new().with_cmd("sysctl -n machdep.cpu.brand_string", "Apple M4 Pro")
    }

    #[test]
    fn llamacpp_binary_present_is_ready() {
        let sys = apple_silicon().with_cmd("llama-server --version", "version: 4589 (a1b2c3d)");
        let d = LlamaCppProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("4589"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Command" && v == "llama-server")
        );
    }

    #[test]
    fn llamacpp_via_homebrew_only_is_ready() {
        let sys = apple_silicon().with_cmd("brew list llama.cpp --versions", "llama.cpp 4589");
        let d = LlamaCppProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("4589"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Installed with Homebrew" && v == "yes")
        );
    }

    #[test]
    fn llamacpp_absent() {
        let d = LlamaCppProbe.detect(&apple_silicon());

        assert_eq!(d.availability, Availability::NotFound);
        assert!(d.version.is_none());
        assert!(
            d.details.is_empty(),
            "nothing installed means nothing to say about it"
        );
    }

    #[test]
    fn falls_back_to_llama_cli_when_the_server_is_missing() {
        let sys = apple_silicon().with_cmd("llama-cli --version", "version: 4600 (deadbee)");
        let d = LlamaCppProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("4600"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Command" && v == "llama-cli")
        );
    }

    /// llama.cpp prints its version banner to stderr, which the OS layer drops.
    /// The command still succeeds, and that success is the install signal — the
    /// probe must not conclude "absent" just because it learned no version.
    #[test]
    fn silent_binary_still_counts_as_installed() {
        let sys = apple_silicon().with_cmd("llama-server --version", "");
        let d = LlamaCppProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert!(d.version.is_none());
    }

    /// Homebrew knows the version even when the binary tells us nothing.
    #[test]
    fn homebrew_supplies_the_version_a_silent_binary_withholds() {
        let sys = apple_silicon()
            .with_cmd("llama-server --version", "")
            .with_cmd("brew list llama.cpp --versions", "llama.cpp 4589");
        let d = LlamaCppProbe.detect(&sys);

        assert_eq!(d.version.as_deref(), Some("4589"));
    }

    #[test]
    fn intel_mac_does_not_claim_graphics_acceleration() {
        let sys = MockSys::new()
            .with_cmd("sysctl -n machdep.cpu.brand_string", "Intel(R) Core(TM) i9")
            .with_cmd("llama-server --version", "version: 4589 (a1b2c3d)");
        let d = LlamaCppProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert!(!d.details.iter().any(|(k, _)| k == "Metal acceleration"));
    }

    #[test]
    fn carries_a_plain_english_explanation() {
        let d = LlamaCppProbe.detect(&MockSys::new());
        assert!(d.friendly.starts_with("llama.cpp — "));
        assert!(d.friendly.len() > 30);
    }
}
