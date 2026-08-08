//! Ollama — the most common way to run AI models locally on a Mac.
//!
//! Two independent signals: the `ollama` command being on PATH means it is
//! installed, and the local API answering means the background service is
//! actually running. Either can be true without the other.

use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

/// Ollama's local API. Only ever contacted on loopback — nothing leaves the Mac.
const TAGS_URL: &str = "http://localhost:11434/api/tags";

/// The service is either listening on loopback or it is not; a slow answer
/// here would just delay the scan for no benefit.
const TAGS_TIMEOUT_MS: u64 = 800;

pub struct OllamaProbe;

impl Probe for OllamaProbe {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        // The command answering at all means installed, even if its output is
        // in a shape we cannot pull a version number out of.
        let raw_version = sys.run("ollama", &["--version"]);
        let installed = raw_version.is_some();
        let version = raw_version.as_deref().and_then(parse_version);

        let tags = sys.http_get(TAGS_URL, TAGS_TIMEOUT_MS);
        let running = tags.is_some();

        let mut details = Vec::new();
        if let Some(body) = &tags {
            let models = model_names(body);
            details.push((
                "Models installed".to_string(),
                if models.is_empty() {
                    // Ollama with no models is a working install with nothing
                    // to run; say so rather than showing an empty cell.
                    "none yet".to_string()
                } else {
                    models.join(", ")
                },
            ));
            details.push(("Local address".to_string(), TAGS_URL.to_string()));
            // The doctor's memory advice needs a size, and most installed
            // models cannot supply one from their name — `codestral:latest` is
            // a 22B model. The API reports the real figure, so carry it here.
            if let Some(largest) = largest_model_size(body) {
                details.push(("Largest model".to_string(), largest));
            }
        }

        // The API answering is what makes Ollama usable. Ollama.app can serve
        // without putting the `ollama` command on PATH, so a running service
        // counts as ready even when the command is missing.
        let availability = if running {
            Availability::Ready
        } else if installed {
            Availability::InstalledNotRunning
        } else {
            Availability::NotFound
        };

        Detection {
            id: "ollama",
            name: "Ollama",
            category: Category::LocalRuntime,
            availability,
            version,
            details,
            friendly: "Ollama — a free app that runs AI models privately on your Mac".to_string(),
        }
    }
}

/// Pull the version out of `ollama version is 0.31.0`.
///
/// The real binary appends a warning line when client and server versions
/// differ, so taking the *first* version-shaped token matters here: the second
/// one is the client's version, not the server's.
fn parse_version(raw: &str) -> Option<String> {
    super::first_version_token(raw)
}

/// Model names from `/api/tags`. A payload we cannot read is treated as no
/// models rather than as a failure — the service is plainly up either way.
fn model_names(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|parsed| {
            let models = parsed.get("models")?.as_array()?;
            Some(
                models
                    .iter()
                    .filter_map(|m| Some(m.get("name")?.as_str()?.to_string()))
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// The biggest installed model, as Ollama reports it (`"22.2B"`, `"137M"`).
///
/// Sizes live in `details.parameter_size` rather than in the name: only some
/// tags spell the size out, and `:latest` never does.
fn largest_model_size(body: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    parsed
        .get("models")?
        .as_array()?
        .iter()
        .filter_map(|m| {
            let raw = m.get("details")?.get("parameter_size")?.as_str()?;
            Some((billions(raw)?, raw.to_string()))
        })
        .max_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, raw)| raw)
}

/// `"22.2B"` becomes 22.2; `"137M"` becomes 0.137. Used only for comparison.
fn billions(raw: &str) -> Option<f32> {
    let (number, unit) = raw.split_at(raw.len().checked_sub(1)?);
    let value: f32 = number.parse().ok()?;
    match unit {
        "B" | "b" => Some(value),
        "M" | "m" => Some(value / 1000.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Availability;
    use crate::sys::testing::MockSys;

    const TAGS: &str = r#"{"models":[{"name":"llama3.2:latest"},{"name":"qwen3:8b"}]}"#;

    #[test]
    fn ollama_running_with_models() {
        let sys = MockSys::new()
            .with_cmd("ollama --version", "ollama version is 0.31.0")
            .with_http("http://localhost:11434/api/tags", TAGS);
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("0.31.0"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Models installed" && v.contains("qwen3"))
        );
    }

    #[test]
    fn ollama_installed_but_stopped() {
        let sys = MockSys::new().with_cmd("ollama --version", "ollama version is 0.31.0");
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::InstalledNotRunning);
        assert_eq!(d.version.as_deref(), Some("0.31.0"));
    }

    #[test]
    fn ollama_absent() {
        let d = OllamaProbe.detect(&MockSys::new());

        assert_eq!(d.availability, Availability::NotFound);
        assert!(d.version.is_none());
    }

    /// Ollama.app can run the server without putting the `ollama` command on
    /// PATH. The API answering is what makes it usable, so that is Ready.
    #[test]
    fn daemon_answering_without_cli_is_ready() {
        let sys = MockSys::new().with_http("http://localhost:11434/api/tags", TAGS);
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
    }

    #[test]
    fn running_with_no_models_says_so_in_plain_words() {
        let sys = MockSys::new()
            .with_cmd("ollama --version", "ollama version is 0.31.0")
            .with_http("http://localhost:11434/api/tags", r#"{"models":[]}"#);
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        let models = d
            .details
            .iter()
            .find(|(k, _)| k == "Models installed")
            .map(|(_, v)| v.clone())
            .expect("should still report the row");
        assert_eq!(models, "none yet");
    }

    #[test]
    fn parses_version_from_real_world_output() {
        // The real binary prints a trailing warning line when the client and
        // server versions differ; the server version comes first.
        for (raw, want) in [
            ("ollama version is 0.31.0", "0.31.0"),
            ("ollama version 0.1.32", "0.1.32"),
            (
                "ollama version is 0.32.1\nWarning: client version is 0.32.6",
                "0.32.1",
            ),
        ] {
            let sys = MockSys::new().with_cmd("ollama --version", raw);
            assert_eq!(
                OllamaProbe.detect(&sys).version.as_deref(),
                Some(want),
                "raw: {raw}"
            );
        }
    }

    #[test]
    fn unparseable_version_still_counts_as_installed() {
        let sys = MockSys::new().with_cmd("ollama --version", "something unexpected");
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::InstalledNotRunning);
        assert!(d.version.is_none());
    }

    #[test]
    fn malformed_tags_payload_does_not_break_the_probe() {
        let sys = MockSys::new()
            .with_cmd("ollama --version", "ollama version is 0.31.0")
            .with_http("http://localhost:11434/api/tags", "not json");
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
    }

    /// Sizes come from the API, not the name. On a real machine most tags are
    /// `:latest`, which says nothing about how big the model is.
    #[test]
    fn reports_the_largest_model_size_even_when_names_hide_it() {
        let body = r#"{"models":[
            {"name":"qwen3:4b","details":{"parameter_size":"4.0B"}},
            {"name":"codestral:latest","details":{"parameter_size":"22.2B"}},
            {"name":"nomic-embed-text:latest","details":{"parameter_size":"137M"}}
        ]}"#;
        let sys = MockSys::new().with_http("http://localhost:11434/api/tags", body);
        let d = OllamaProbe.detect(&sys);

        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Largest model" && v == "22.2B"),
            "details were {:?}",
            d.details
        );
    }

    /// Millions must not outrank billions on a plain string comparison.
    #[test]
    fn model_sizes_are_compared_numerically() {
        let body = r#"{"models":[
            {"name":"small:latest","details":{"parameter_size":"900M"}},
            {"name":"big:latest","details":{"parameter_size":"7.0B"}}
        ]}"#;
        let sys = MockSys::new().with_http("http://localhost:11434/api/tags", body);
        let d = OllamaProbe.detect(&sys);

        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Largest model" && v == "7.0B")
        );
    }

    /// Older payloads without a size must not break the probe or invent a row.
    #[test]
    fn missing_size_information_is_simply_omitted() {
        let sys = MockSys::new().with_http(
            "http://localhost:11434/api/tags",
            r#"{"models":[{"name":"mystery:latest"}]}"#,
        );
        let d = OllamaProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert!(!d.details.iter().any(|(k, _)| k == "Largest model"));
    }

    /// The plain-language rule: anyone who has never heard of Ollama should
    /// learn what it is from the detection alone.
    #[test]
    fn carries_a_plain_english_explanation() {
        let d = OllamaProbe.detect(&MockSys::new());
        assert!(d.friendly.starts_with("Ollama — "));
        assert!(d.friendly.len() > 30);
    }
}
