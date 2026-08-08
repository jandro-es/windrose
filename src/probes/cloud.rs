//! Cloud AI services — models that run on someone else's computer.
//!
//! Every provider is detected the same way, so there is one probe driven by a
//! table rather than six near-identical files.
//!
//! **Secrets rule.** Credentials are detected by presence alone. Nothing here
//! reads, stores, logs or renders a key's value; the detail rows are plain
//! yes/no answers and paths.

use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

/// One cloud service and the traces it leaves on a Mac.
struct Provider {
    id: &'static str,
    name: &'static str,
    /// Command-line tool, checked with `--version`.
    cli: Option<&'static str>,
    /// Variables that would hold an API key. Presence only — never the value.
    env_vars: &'static [&'static str],
    /// Desktop app bundle.
    app: Option<&'static str>,
    /// Settings folder under the user's home, created once the tool is set up.
    config_dir: Option<&'static str>,
    friendly: &'static str,
}

/// The services Windrose knows how to look for.
const PROVIDERS: &[Provider] = &[
    Provider {
        id: "claude",
        name: "Claude",
        cli: Some("claude"),
        env_vars: &["ANTHROPIC_API_KEY"],
        app: Some("/Applications/Claude.app"),
        config_dir: Some(".claude"),
        friendly: "Claude — Anthropic's AI service, used through the Claude app or the \
                   `claude` command",
    },
    Provider {
        id: "openai",
        name: "OpenAI",
        cli: Some("codex"),
        env_vars: &["OPENAI_API_KEY"],
        app: Some("/Applications/ChatGPT.app"),
        config_dir: Some(".codex"),
        friendly: "OpenAI — the company behind ChatGPT, used through the ChatGPT app or the \
                   `codex` command",
    },
    Provider {
        id: "gemini",
        name: "Gemini",
        cli: Some("gemini"),
        env_vars: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        app: None,
        config_dir: Some(".gemini"),
        friendly: "Gemini — Google's AI service, used through the `gemini` command",
    },
    Provider {
        id: "perplexity",
        name: "Perplexity",
        cli: None,
        env_vars: &["PERPLEXITY_API_KEY", "PPLX_API_KEY"],
        app: Some("/Applications/Perplexity.app"),
        config_dir: None,
        friendly: "Perplexity — an AI service that answers questions and cites its sources",
    },
    Provider {
        id: "mistral",
        name: "Mistral",
        cli: None,
        env_vars: &["MISTRAL_API_KEY"],
        app: None,
        config_dir: None,
        friendly: "Mistral — a European AI service, used through its website or an API key",
    },
    Provider {
        id: "groq",
        name: "Groq",
        cli: None,
        env_vars: &["GROQ_API_KEY"],
        app: None,
        config_dir: None,
        friendly: "Groq — a service that runs freely available AI models unusually fast",
    },
];

pub struct CloudProbe {
    provider: &'static Provider,
}

/// One probe per known service, in table order.
pub fn probes() -> Vec<CloudProbe> {
    PROVIDERS
        .iter()
        .map(|provider| CloudProbe { provider })
        .collect()
}

impl Probe for CloudProbe {
    fn id(&self) -> &'static str {
        self.provider.id
    }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        let p = self.provider;

        let cli_version = p.cli.and_then(|cli| sys.run(cli, &["--version"]));
        let cli_present = cli_version.is_some();
        let app_present = p.app.is_some_and(|app| sys.path_exists(app));
        let key_in_env = p.env_vars.iter().any(|var| sys.env_is_set(var));
        let settings = p
            .config_dir
            .map(|dir| sys.home().join(dir).to_string_lossy().into_owned())
            .filter(|path| sys.path_exists(path));

        let tool_present = cli_present || app_present;
        // A signed-in tool keeps its session in its settings folder. Treating
        // only an environment key as "configured" would mark a perfectly
        // working subscription sign-in as half-finished.
        let credential = key_in_env || settings.is_some();

        let mut details = Vec::new();
        if !p.env_vars.is_empty() {
            details.push((
                "API key found in environment".to_string(),
                yes_no(key_in_env),
            ));
        }
        if let Some(cli) = p.cli {
            details.push((
                "Command-line tool".to_string(),
                installed_or_not(cli_present, cli),
            ));
        }
        if p.app.is_some() {
            details.push(("Desktop app".to_string(), yes_no(app_present)));
        }
        if let Some(path) = &settings {
            details.push(("Settings folder".to_string(), path.clone()));
        }

        let availability = match (tool_present, credential) {
            (true, true) => Availability::Ready,
            (false, true) => Availability::Partial("API key set, no tools installed".to_string()),
            (true, false) => {
                Availability::Partial("Tool installed, but no sign-in or API key found".to_string())
            }
            (false, false) => Availability::NotFound,
        };

        Detection {
            id: p.id,
            name: p.name,
            category: Category::CloudProvider,
            availability,
            version: cli_version.as_deref().and_then(super::first_version_token),
            details,
            friendly: p.friendly.to_string(),
        }
    }
}

fn yes_no(present: bool) -> String {
    if present { "yes" } else { "no" }.to_string()
}

fn installed_or_not(present: bool, name: &str) -> String {
    if present {
        format!("{name} — installed")
    } else {
        format!("{name} — not installed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    fn probe(id: &str) -> CloudProbe {
        probes()
            .into_iter()
            .find(|p| p.id() == id)
            .unwrap_or_else(|| panic!("no provider with id {id}"))
    }

    #[test]
    fn claude_fully_configured() {
        let sys = MockSys::new()
            .with_cmd("claude --version", "2.1.0 (Claude Code)")
            .with_env("ANTHROPIC_API_KEY")
            .with_path("/Applications/Claude.app");
        let d = probe("claude").detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("2.1.0"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "API key found in environment" && v == "yes")
        );
    }

    #[test]
    fn openai_key_only_is_partial() {
        let sys = MockSys::new().with_env("OPENAI_API_KEY");
        let d = probe("openai").detect(&sys);

        assert_eq!(
            d.availability,
            Availability::Partial("API key set, no tools installed".to_string())
        );
    }

    #[test]
    fn groq_absent() {
        let d = probe("groq").detect(&MockSys::new());

        assert_eq!(d.availability, Availability::NotFound);
        assert!(d.version.is_none());
    }

    /// The common real-world setup: the tool is signed in to a subscription
    /// and keeps its session in its own settings folder, with no API key
    /// anywhere in the environment. That is fully working, not half configured.
    #[test]
    fn signed_in_tool_without_an_environment_key_is_ready() {
        let sys = MockSys::new()
            .with_home("/Users/someone")
            .with_cmd("claude --version", "2.1.226 (Claude Code)")
            .with_path("/Users/someone/.claude");
        let d = probe("claude").detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "API key found in environment" && v == "no"),
            "the key really is absent and the report should say so"
        );
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Settings folder" && v == "/Users/someone/.claude")
        );
    }

    /// Installed but never set up: no key, no settings folder.
    #[test]
    fn tool_without_any_credential_is_partial() {
        let sys = MockSys::new().with_cmd("claude --version", "2.1.226 (Claude Code)");
        let d = probe("claude").detect(&sys);

        assert_eq!(
            d.availability,
            Availability::Partial("Tool installed, but no sign-in or API key found".to_string())
        );
    }

    /// Secrets rule: a key's value must never reach a `Detection`, whatever
    /// shape it takes. Only presence is ever reported.
    #[test]
    fn no_detail_row_can_carry_a_key_value() {
        let sys = MockSys::new()
            .with_home("/Users/someone")
            .with_cmd("claude --version", "2.1.226 (Claude Code)")
            .with_env("ANTHROPIC_API_KEY")
            .with_path("/Applications/Claude.app")
            .with_path("/Users/someone/.claude");

        for p in probes() {
            let d = p.detect(&sys);
            for (key, value) in &d.details {
                assert!(
                    !value.contains("sk-") && !value.contains("sk_"),
                    "{}: row '{key}' leaked a key-shaped value",
                    d.id
                );
                // The only way a value could leak is by being read at all.
                // Presence rows are exactly these two words.
                if key == "API key found in environment" {
                    assert!(value == "yes" || value == "no", "{}: {value}", d.id);
                }
            }
        }
    }

    /// Google's tooling reads either variable, so either one counts.
    #[test]
    fn gemini_accepts_either_key_variable() {
        for var in ["GEMINI_API_KEY", "GOOGLE_API_KEY"] {
            let sys = MockSys::new().with_env(var);
            assert_eq!(
                probe("gemini").detect(&sys).availability,
                Availability::Partial("API key set, no tools installed".to_string()),
                "{var} should be recognised"
            );
        }
    }

    /// A desktop app with a signed-in account and no CLI is still usable.
    #[test]
    fn app_only_with_a_key_is_ready() {
        let sys = MockSys::new()
            .with_path("/Applications/Perplexity.app")
            .with_env("PPLX_API_KEY");
        let d = probe("perplexity").detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
    }

    #[test]
    fn parses_each_tools_real_version_output() {
        for (id, cli, raw, want) in [
            ("claude", "claude", "2.1.226 (Claude Code)", "2.1.226"),
            ("openai", "codex", "codex-cli 0.147.0", "0.147.0"),
            ("gemini", "gemini", "0.1.7", "0.1.7"),
        ] {
            let sys = MockSys::new().with_cmd(&format!("{cli} --version"), raw);
            assert_eq!(
                probe(id).detect(&sys).version.as_deref(),
                Some(want),
                "raw: {raw}"
            );
        }
    }

    #[test]
    fn the_expected_six_providers_are_present() {
        let ids: Vec<_> = probes().iter().map(|p| p.id()).collect();
        assert_eq!(
            ids,
            [
                "claude",
                "openai",
                "gemini",
                "perplexity",
                "mistral",
                "groq"
            ]
        );
    }

    /// The plain-language rule applies to every one of them.
    #[test]
    fn every_provider_explains_itself_in_plain_english() {
        for p in probes() {
            let d = p.detect(&MockSys::new());
            assert!(
                d.friendly.starts_with(&format!("{} — ", d.name)),
                "{}: {}",
                d.id,
                d.friendly
            );
            assert!(d.friendly.len() > 30, "{}: too terse", d.id);
        }
    }
}
