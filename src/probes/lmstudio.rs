//! LM Studio — the point-and-click way to run AI models locally.
//!
//! Three independent signals: the app bundle in `/Applications`, the `lms`
//! command-line helper, and the OpenAI-compatible server on port 1234. The app
//! can be installed without the CLI, and the server only runs when the user
//! starts it.

use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

/// Where the app installs itself.
const APP_BUNDLE: &str = "/Applications/LM Studio.app";

/// The app's own version metadata. `lms version` prints a banner and a git
/// commit but no version number, so the bundle is the only reliable source.
const APP_PLIST: &str = "/Applications/LM Studio.app/Contents/Info.plist";

/// LM Studio's local server speaks the OpenAI API. Loopback only.
const MODELS_URL: &str = "http://localhost:1234/v1/models";

/// The server is either listening or it is not; waiting longer buys nothing.
const MODELS_TIMEOUT_MS: u64 = 800;

pub struct LmStudioProbe;

impl Probe for LmStudioProbe {
    fn id(&self) -> &'static str {
        "lmstudio"
    }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        let app_installed = sys.path_exists(APP_BUNDLE);
        // Presence only. The output is an ASCII-art banner with no version in
        // it, so there is nothing here worth parsing.
        let cli_installed = sys.run("lms", &["version"]).is_some();

        let version = sys
            .run(
                "defaults",
                &["read", APP_PLIST, "CFBundleShortVersionString"],
            )
            .filter(|v| !v.is_empty());

        let served = sys.http_get(MODELS_URL, MODELS_TIMEOUT_MS);

        let mut details = Vec::new();
        if let Some(body) = &served {
            let models = model_names(body);
            details.push((
                "Models loaded".to_string(),
                if models.is_empty() {
                    "none yet".to_string()
                } else {
                    models.join(", ")
                },
            ));
            details.push(("Local address".to_string(), MODELS_URL.to_string()));
        }
        if let Some(dir) = models_dir(sys) {
            details.push(("Models folder".to_string(), dir));
        }

        // The server answering is what makes LM Studio usable from other tools.
        // Either half of the install on its own means "here but not serving".
        let availability = if served.is_some() {
            Availability::Ready
        } else if app_installed || cli_installed {
            Availability::InstalledNotRunning
        } else {
            Availability::NotFound
        };

        Detection {
            id: "lmstudio",
            name: "LM Studio",
            category: Category::LocalRuntime,
            availability,
            version,
            details,
            friendly: "LM Studio — a point-and-click app for downloading and chatting with local \
                       AI models"
                .to_string(),
        }
    }
}

/// Where LM Studio keeps downloaded models. Older installs used a path under
/// `~/.cache`, so that is checked as a fallback.
fn models_dir(sys: &dyn SysCtx) -> Option<String> {
    [".lmstudio/models", ".cache/lm-studio/models"]
        .into_iter()
        .map(|rel| sys.home().join(rel).to_string_lossy().into_owned())
        .find(|path| sys.path_exists(path))
}

/// Loaded model names from the OpenAI-shaped `/v1/models` response. A payload
/// we cannot read is treated as no models rather than as a failure — the
/// server is plainly up either way.
fn model_names(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|parsed| {
            let data = parsed.get("data")?.as_array()?;
            Some(
                data.iter()
                    .filter_map(|m| Some(m.get("id")?.as_str()?.to_string()))
                    .collect(),
            )
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    const MODELS: &str = r#"{"data":[{"id":"qwen3-8b"},{"id":"gemma-3-12b"}]}"#;

    /// The plist read, spelled the way `MockSys` keys commands.
    const PLIST_CMD: &str =
        "defaults read /Applications/LM Studio.app/Contents/Info.plist CFBundleShortVersionString";

    #[test]
    fn lmstudio_server_running_is_ready() {
        let sys = MockSys::new()
            .with_path("/Applications/LM Studio.app")
            .with_cmd("lms version", "banner")
            .with_cmd(PLIST_CMD, "0.4.20+1")
            .with_http("http://localhost:1234/v1/models", MODELS);
        let d = LmStudioProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("0.4.20+1"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Models loaded" && v.contains("gemma-3-12b"))
        );
    }

    #[test]
    fn lmstudio_installed_but_server_stopped() {
        let sys = MockSys::new()
            .with_path("/Applications/LM Studio.app")
            .with_cmd(PLIST_CMD, "0.4.20+1");
        let d = LmStudioProbe.detect(&sys);

        assert_eq!(d.availability, Availability::InstalledNotRunning);
        assert_eq!(d.version.as_deref(), Some("0.4.20+1"));
    }

    #[test]
    fn lmstudio_absent() {
        let d = LmStudioProbe.detect(&MockSys::new());

        assert_eq!(d.availability, Availability::NotFound);
        assert!(d.version.is_none());
        assert!(d.details.is_empty());
    }

    /// `lms` installs into `~/.lmstudio/bin`, so it can be on PATH after the
    /// app bundle has been moved or removed.
    #[test]
    fn cli_without_the_app_bundle_still_counts_as_installed() {
        let sys = MockSys::new().with_cmd("lms version", "banner");
        let d = LmStudioProbe.detect(&sys);

        assert_eq!(d.availability, Availability::InstalledNotRunning);
        assert!(d.version.is_none(), "no bundle means no version to read");
    }

    /// `lms version` prints an ASCII-art banner and a git commit hash — never a
    /// version number. Reading it as a version would put "CLI commit: 71bd99c"
    /// (or a slice of the artwork) in front of the user.
    #[test]
    fn version_comes_from_the_app_bundle_not_the_cli() {
        let banner = "lms is LM Studio's CLI utility for your models, server, and inference \
                      runtime.\nCLI commit: 71bd99c";
        let sys = MockSys::new()
            .with_path("/Applications/LM Studio.app")
            .with_cmd("lms version", banner)
            .with_cmd(PLIST_CMD, "0.4.20+1");
        let d = LmStudioProbe.detect(&sys);

        assert_eq!(d.version.as_deref(), Some("0.4.20+1"));
    }

    #[test]
    fn models_folder_is_reported_when_present() {
        let sys = MockSys::new()
            .with_home("/Users/someone")
            .with_path("/Applications/LM Studio.app")
            .with_path("/Users/someone/.lmstudio/models");
        let d = LmStudioProbe.detect(&sys);

        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Models folder" && v == "/Users/someone/.lmstudio/models")
        );
    }

    #[test]
    fn models_folder_falls_back_to_the_cache_location() {
        let sys = MockSys::new()
            .with_home("/Users/someone")
            .with_path("/Applications/LM Studio.app")
            .with_path("/Users/someone/.cache/lm-studio/models");
        let d = LmStudioProbe.detect(&sys);

        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Models folder" && v == "/Users/someone/.cache/lm-studio/models")
        );
    }

    #[test]
    fn server_with_no_models_says_so_in_plain_words() {
        let sys = MockSys::new()
            .with_path("/Applications/LM Studio.app")
            .with_http("http://localhost:1234/v1/models", r#"{"data":[]}"#);
        let d = LmStudioProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Models loaded" && v == "none yet")
        );
    }

    #[test]
    fn malformed_models_payload_does_not_break_the_probe() {
        let sys = MockSys::new()
            .with_path("/Applications/LM Studio.app")
            .with_http("http://localhost:1234/v1/models", "not json");
        let d = LmStudioProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
    }

    #[test]
    fn carries_a_plain_english_explanation() {
        let d = LmStudioProbe.detect(&MockSys::new());
        assert!(d.friendly.starts_with("LM Studio — "));
        assert!(d.friendly.len() > 30);
    }
}
