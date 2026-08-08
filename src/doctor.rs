//! Turning a scan into advice.
//!
//! Two questions, answered separately: *is anything broken or half-finished?*
//! (health) and *could this Mac do better?* (performance).
//!
//! **Windrose never changes the machine.** Every fix is returned as a
//! [`FixGuide`] — numbered steps for someone who has never opened Terminal,
//! plus the exact commands to copy. Running them is always the user's choice,
//! and frontends ask before showing setup flows at all.

use crate::hardware::{ChipTier, HardwareProfile};
use crate::model::{Availability, Detection};
use crate::sys::SysCtx;

/// Free disk space below which model downloads become a problem.
const MIN_FREE_GB: u32 = 20;

/// Share of memory a model can realistically use. Matches `scoring.rs`.
const USABLE_MEMORY_SHARE: f32 = 0.65;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CheckStatus {
    /// Nothing to do.
    Pass,
    /// Works, but something is worth improving.
    Warn,
    /// Does not work as things stand.
    Fail,
}

/// How to fix one finding, for both kinds of reader.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FixGuide {
    /// Numbered, plain-English instructions assuming no prior knowledge.
    pub steps: Vec<String>,
    /// The same thing as commands to copy and paste.
    pub commands: Vec<String>,
    pub docs_url: Option<&'static str>,
}

/// One finding.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub title: String,
    pub status: CheckStatus,
    /// What this means, in plain words, and why it matters.
    pub explanation: String,
    /// Present whenever there is something the user could do.
    pub fix: Option<FixGuide>,
}

impl CheckResult {
    fn new(id: &'static str, title: &str, status: CheckStatus, explanation: String) -> Self {
        Self {
            id,
            title: title.to_string(),
            status,
            explanation,
            fix: None,
        }
    }

    fn with_fix(
        mut self,
        steps: &[&str],
        commands: &[&str],
        docs_url: Option<&'static str>,
    ) -> Self {
        self.fix = Some(FixGuide {
            steps: steps.iter().map(|s| (*s).to_string()).collect(),
            commands: commands.iter().map(|c| (*c).to_string()).collect(),
            docs_url,
        });
        self
    }
}

/// A local runtime and how someone would install or start it.
struct RuntimeGuide {
    id: &'static str,
    name: &'static str,
    install_steps: &'static [&'static str],
    install_commands: &'static [&'static str],
    start_steps: &'static [&'static str],
    start_commands: &'static [&'static str],
    docs_url: &'static str,
}

/// Opening line for anyone who has never used Terminal. Repeated per guide so
/// that a single copied guide still makes sense on its own.
const OPEN_TERMINAL: &str =
    "Open Terminal: press Command-Space, type \"Terminal\", and press Return.";

const RUNTIME_GUIDES: &[RuntimeGuide] = &[
    RuntimeGuide {
        id: "ollama",
        name: "Ollama",
        install_steps: &[
            OPEN_TERMINAL,
            "Copy the command below, paste it into Terminal, and press Return.",
            "Wait for it to finish — it downloads a few hundred megabytes.",
            "Run \"ollama serve\" to start it, then \"ollama pull llama3.2\" to get your first model.",
        ],
        install_commands: &[
            "brew install ollama",
            "ollama serve",
            "ollama pull llama3.2",
        ],
        start_steps: &[
            OPEN_TERMINAL,
            "Run the first command to start Ollama for this session.",
            "Use the second command instead if you would like it to start automatically from now on.",
        ],
        start_commands: &["ollama serve", "brew services start ollama"],
        docs_url: "https://ollama.com/download",
    },
    RuntimeGuide {
        id: "lmstudio",
        name: "LM Studio",
        install_steps: &[
            OPEN_TERMINAL,
            "Run the command below to install the LM Studio app.",
            "Open LM Studio from your Applications folder and pick a model to download.",
        ],
        install_commands: &["brew install --cask lm-studio"],
        start_steps: &[
            "Open LM Studio from your Applications folder.",
            "Go to the Developer tab and switch the local server on if you want other apps to use it.",
        ],
        start_commands: &["open -a \"LM Studio\""],
        docs_url: "https://lmstudio.ai/docs",
    },
    RuntimeGuide {
        id: "llamacpp",
        name: "llama.cpp",
        install_steps: &[OPEN_TERMINAL, "Run the command below to install llama.cpp."],
        install_commands: &["brew install llama.cpp"],
        start_steps: &["llama.cpp runs on demand — there is no service to start."],
        start_commands: &[],
        docs_url: "https://github.com/ggml-org/llama.cpp",
    },
    RuntimeGuide {
        id: "mlx",
        name: "MLX",
        install_steps: &[
            OPEN_TERMINAL,
            "Run the command below to install Apple's MLX toolkit for language models.",
            "This needs Python 3, which macOS already includes.",
        ],
        install_commands: &["pip3 install mlx-lm"],
        start_steps: &["MLX runs on demand — there is no service to start."],
        start_commands: &[],
        docs_url: "https://github.com/ml-explore/mlx-lm",
    },
];

/// Is anything broken, missing or half-finished?
///
/// **Deviation from the plan's signature:** this takes `sys` because the
/// prerequisite Homebrew check the plan asks for cannot be answered from
/// detections alone. Everything reading the OS goes through `SysCtx`.
pub fn health_checks(
    dets: &[Detection],
    _hw: &HardwareProfile,
    sys: &dyn SysCtx,
) -> Vec<CheckResult> {
    let mut checks = vec![homebrew_check(sys)];

    for guide in RUNTIME_GUIDES {
        if let Some(det) = find(dets, guide.id) {
            checks.push(runtime_check(guide, det));
        }
    }
    checks.extend(ollama_models_check(dets));
    checks.extend(apple_checks(dets));
    checks.extend(cloud_checks(dets));
    checks
}

/// Could this Mac do better than it currently is?
///
/// Takes `sys` for the same reason as [`health_checks`] — the free-space check
/// the plan asks for reads the disk.
// Consumed by `gather()` in Task 11; remove this allow when that lands.
#[allow(dead_code)]
pub fn performance_checks(
    dets: &[Detection],
    hw: &HardwareProfile,
    sys: &dyn SysCtx,
) -> Vec<CheckResult> {
    let mut checks = vec![memory_check(dets, hw), disk_check(sys)];
    checks.extend(ollama_tuning_check(dets));
    checks.extend(mlx_preference_check(dets, hw));
    checks.extend(laptop_thermal_check(hw));
    checks.extend(apple_fm_unused_check(dets, hw));
    checks
}

// ---------------------------------------------------------------- health ---

/// Homebrew comes first: nearly every other fix is a `brew install`, so a
/// missing Homebrew would make the rest of the advice unfollowable.
fn homebrew_check(sys: &dyn SysCtx) -> CheckResult {
    if sys.run("brew", &["--version"]).is_some() {
        return CheckResult::new(
            "homebrew-installed",
            "Homebrew",
            CheckStatus::Pass,
            "Homebrew — the tool that installs other software from the Terminal — is ready."
                .to_string(),
        );
    }

    CheckResult::new(
        "homebrew-installed",
        "Homebrew",
        CheckStatus::Fail,
        "Homebrew is a free tool that installs other software from the Terminal. Most of the \
         other suggestions below need it, so it is worth setting up first."
            .to_string(),
    )
    .with_fix(
        &[
            OPEN_TERMINAL,
            "Copy the command below, paste it into Terminal, and press Return.",
            "It will ask for your Mac password, then take a few minutes.",
            "Close Terminal and open it again so the new command is available.",
        ],
        &["/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""],
        Some("https://brew.sh"),
    )
}

fn runtime_check(guide: &RuntimeGuide, det: &Detection) -> CheckResult {
    let id: &'static str = match guide.id {
        "ollama" => "ollama-installed",
        "lmstudio" => "lmstudio-installed",
        "llamacpp" => "llamacpp-installed",
        _ => "mlx-installed",
    };

    match &det.availability {
        Availability::Ready => CheckResult::new(
            id,
            guide.name,
            CheckStatus::Pass,
            format!("{} is installed and ready to use.", guide.name),
        ),
        Availability::InstalledNotRunning => CheckResult::new(
            id,
            guide.name,
            CheckStatus::Warn,
            format!(
                "{} is installed but not currently running, so nothing can use it yet.",
                guide.name
            ),
        )
        .with_fix(
            guide.start_steps,
            guide.start_commands,
            Some(guide.docs_url),
        ),
        Availability::Partial(reason) => CheckResult::new(
            id,
            guide.name,
            CheckStatus::Warn,
            format!("{} is partly set up: {reason}.", guide.name),
        )
        .with_fix(
            guide.install_steps,
            guide.install_commands,
            Some(guide.docs_url),
        ),
        Availability::NotFound => CheckResult::new(
            id,
            guide.name,
            CheckStatus::Fail,
            format!(
                "{} — {}. It is not installed on this Mac.",
                guide.name,
                short(det)
            ),
        )
        .with_fix(
            guide.install_steps,
            guide.install_commands,
            Some(guide.docs_url),
        ),
    }
}

/// A working Ollama with nothing to run is a common half-finished state.
fn ollama_models_check(dets: &[Detection]) -> Option<CheckResult> {
    let det = find(dets, "ollama")?;
    if det.availability != Availability::Ready {
        return None;
    }

    let has_models = detail(det, "Models installed").is_some_and(|v| v != "none yet");
    if has_models {
        return Some(CheckResult::new(
            "ollama-has-models",
            "Ollama models",
            CheckStatus::Pass,
            "Ollama has at least one model downloaded.".to_string(),
        ));
    }

    Some(
        CheckResult::new(
            "ollama-has-models",
            "Ollama models",
            CheckStatus::Warn,
            "Ollama is running but has no models yet, so there is nothing for it to answer with."
                .to_string(),
        )
        .with_fix(
            &[
                OPEN_TERMINAL,
                "Run the command below to download a small, general-purpose model to start with.",
                "It is about 2 GB and only needs downloading once.",
            ],
            &["ollama pull llama3.2"],
            Some("https://ollama.com/library"),
        ),
    )
}

fn apple_checks(dets: &[Detection]) -> Vec<CheckResult> {
    let Some(det) = find(dets, "apple-fm") else {
        return Vec::new();
    };

    let mut checks = Vec::new();
    checks.push(match &det.availability {
        Availability::Ready => CheckResult::new(
            "apple-fm-available",
            "Apple's built-in model",
            CheckStatus::Pass,
            "This Mac has Apple's own AI model built into macOS, ready for apps to use."
                .to_string(),
        ),
        Availability::Partial(reason) => CheckResult::new(
            "apple-fm-available",
            "Apple's built-in model",
            CheckStatus::Warn,
            format!("Apple's built-in model is not usable here: {reason}."),
        ),
        _ => CheckResult::new(
            "apple-fm-available",
            "Apple's built-in model",
            CheckStatus::Warn,
            "This Mac's version of macOS does not include Apple's built-in AI model. Everything \
             else here works regardless — this is an extra, not a requirement."
                .to_string(),
        )
        .with_fix(
            &[
                "Open System Settings from the Apple menu.",
                "Go to General, then Software Update.",
                "Install macOS 26 or later if it is offered for this Mac.",
            ],
            &[],
            Some("https://support.apple.com/en-us/102662"),
        ),
    });

    // Only worth asking about once the model is actually present.
    if det.availability == Availability::Ready
        && detail(det, "Apple Intelligence").is_some_and(|v| v.starts_with("Unknown"))
    {
        checks.push(
            CheckResult::new(
                "apple-intelligence-on",
                "Apple Intelligence",
                CheckStatus::Warn,
                "Windrose could not tell whether Apple Intelligence is switched on. Some \
                 features need it, and it is quick to check."
                    .to_string(),
            )
            .with_fix(
                &[
                    "Open System Settings from the Apple menu.",
                    "Click \"Apple Intelligence & Siri\".",
                    "Switch Apple Intelligence on if it is not already.",
                ],
                &[],
                Some("https://support.apple.com/en-us/121115"),
            ),
        );
    }
    checks
}

/// Cloud services are optional by nature, so an unused one is not a failure.
/// Only services showing some sign of use get their own check; the aggregate
/// below covers the case where nothing at all is set up.
fn cloud_checks(dets: &[Detection]) -> Vec<CheckResult> {
    // Each provider needs its own check id: two half-configured services
    // sharing one id would collide in the wizard, which addresses findings by
    // id. The ids are `&'static str`, so they are spelled out here.
    const CLOUD_IDS: [(&str, &str); 6] = [
        ("claude", "claude-configured"),
        ("openai", "openai-configured"),
        ("gemini", "gemini-configured"),
        ("perplexity", "perplexity-configured"),
        ("mistral", "mistral-configured"),
        ("groq", "groq-configured"),
    ];

    let cloud: Vec<(&'static str, &Detection)> = CLOUD_IDS
        .iter()
        .filter_map(|(id, check_id)| find(dets, id).map(|det| (*check_id, det)))
        .collect();
    if cloud.is_empty() {
        return Vec::new();
    }

    let mut checks = Vec::new();
    for (check_id, det) in &cloud {
        if let Availability::Partial(reason) = &det.availability {
            checks.push(
                CheckResult::new(
                    check_id,
                    det.name,
                    CheckStatus::Warn,
                    format!(
                        "{} is only half set up: {reason}. Until that is finished it cannot be \
                         used, though nothing else on this Mac is affected by it.",
                        det.name
                    ),
                )
                .with_fix(
                    &[
                        "Open the app or run the command for this service and sign in — most \
                         services work on a subscription without any key.",
                        "If you use an API key instead, add it to your shell profile so it is \
                         set every time you open Terminal.",
                        "Windrose only ever checks whether a key exists. It never reads, stores \
                         or displays the key itself.",
                    ],
                    &[],
                    None,
                ),
            );
        }
    }

    let any_ready = cloud
        .iter()
        .any(|(_, d)| d.availability == Availability::Ready);
    checks.push(if any_ready {
        CheckResult::new(
            "cloud-access",
            "Cloud services",
            CheckStatus::Pass,
            "At least one cloud AI service is set up, so the largest models are available to you."
                .to_string(),
        )
    } else {
        CheckResult::new(
            "cloud-access",
            "Cloud services",
            CheckStatus::Warn,
            "No cloud AI service is set up. That is fine if you only want to run models on this \
             Mac, but the very largest models are only available online."
                .to_string(),
        )
    });
    checks
}

// ----------------------------------------------------------- performance ---

/// Memory is the usual limit, so this check leads.
fn memory_check(dets: &[Detection], hw: &HardwareProfile) -> CheckResult {
    let usable = hw.ram_gb as f32 * USABLE_MEMORY_SHARE;
    let installed = find(dets, "ollama").and_then(|d| detail(d, "Largest model"));

    // Every model worth running locally is compressed; saying so once, in
    // plain words, is more useful than naming formats.
    let advice = format!(
        "With {} GB of memory, roughly {usable:.0} GB is available to a model. Prefer models \
         published in 4-bit form — they are compressed to about a quarter of their original size, \
         which is how models are normally run on a Mac.",
        hw.ram_gb
    );

    let (status, explanation) = match installed {
        Some(size) => {
            let too_big = size_in_billions(size).is_some_and(|b| b * 0.6 > usable);
            if too_big {
                (
                    CheckStatus::Warn,
                    format!(
                        "The largest model installed is {size}, which is close to or beyond what \
                         this Mac can hold. {advice} A smaller model will be far quicker."
                    ),
                )
            } else {
                (
                    CheckStatus::Pass,
                    format!("The largest model installed is {size}, which fits here. {advice}"),
                )
            }
        }
        None if hw.ram_gb < 16 => (
            CheckStatus::Warn,
            format!("{advice} Stay with 3B-class models on this Mac and close other apps first."),
        ),
        None => (CheckStatus::Pass, advice),
    };

    CheckResult::new("ram-headroom", "Memory for models", status, explanation)
}

fn disk_check(sys: &dyn SysCtx) -> CheckResult {
    let free = sys.run("df", &["-g", "/"]).and_then(|out| free_gb(&out));

    match free {
        Some(gb) if gb < MIN_FREE_GB => CheckResult::new(
            "disk-space",
            "Free disk space",
            CheckStatus::Warn,
            format!(
                "Only {gb} GB of disk space is free. Models are large — a mid-sized one is 5–20 \
                 GB — so downloads may fail."
            ),
        )
        .with_fix(
            &[
                "Open the Apple menu, choose System Settings, then General, then Storage.",
                "Use the recommendations there to free up space.",
                "Deleting models you no longer use is often the quickest win.",
            ],
            &["ollama list", "ollama rm <model-name>"],
            None,
        ),
        Some(gb) => CheckResult::new(
            "disk-space",
            "Free disk space",
            CheckStatus::Pass,
            format!("{gb} GB of disk space is free — plenty of room for model downloads."),
        ),
        None => CheckResult::new(
            "disk-space",
            "Free disk space",
            CheckStatus::Pass,
            "Windrose could not read the free disk space on this Mac.".to_string(),
        ),
    }
}

fn ollama_tuning_check(dets: &[Detection]) -> Option<CheckResult> {
    let det = find(dets, "ollama")?;
    if det.availability != Availability::Ready {
        return None;
    }

    Some(
        CheckResult::new(
            "ollama-tuning",
            "Ollama speed settings",
            CheckStatus::Pass,
            "Two settings make Ollama feel faster. Keeping a model in memory avoids a pause of \
             several seconds every time you come back to it, and a shorter conversation limit \
             uses less memory on long chats."
                .to_string(),
        )
        .with_fix(
            &[
                OPEN_TERMINAL,
                "The first command keeps a model loaded for 30 minutes after you last used it.",
                "The second sets how much conversation the model keeps in mind; lower it if you \
                 run short of memory.",
                "Add these to your shell profile to make them permanent.",
            ],
            &[
                "export OLLAMA_KEEP_ALIVE=30m",
                "export OLLAMA_CONTEXT_LENGTH=4096",
            ],
            Some("https://github.com/ollama/ollama/blob/main/docs/faq.md"),
        ),
    )
}

/// MLX is Apple's own, and is usually faster than llama.cpp on Apple Silicon.
fn mlx_preference_check(dets: &[Detection], hw: &HardwareProfile) -> Option<CheckResult> {
    if !hw.is_apple_silicon {
        return None;
    }
    let llamacpp_ready =
        find(dets, "llamacpp").is_some_and(|d| d.availability == Availability::Ready);
    let mlx_ready = find(dets, "mlx").is_some_and(|d| d.availability == Availability::Ready);
    if !llamacpp_ready || mlx_ready {
        return None;
    }

    Some(
        CheckResult::new(
            "mlx-preferred",
            "A faster option for this chip",
            CheckStatus::Warn,
            "This Mac runs llama.cpp, which works well. Apple's own MLX toolkit is usually \
             faster on Apple Silicon for models that support it, and the two can live side by \
             side."
                .to_string(),
        )
        .with_fix(
            &[
                OPEN_TERMINAL,
                "Run the command below to install MLX's language-model toolkit.",
                "Keep llama.cpp as well — some models are only available for it.",
            ],
            &["pip3 install mlx-lm"],
            Some("https://github.com/ml-explore/mlx-lm"),
        ),
    )
}

fn laptop_thermal_check(hw: &HardwareProfile) -> Option<CheckResult> {
    if !hw.is_laptop || !matches!(hw.chip_tier, ChipTier::Max | ChipTier::Ultra) {
        return None;
    }

    Some(CheckResult::new(
        "laptop-thermals",
        "Long runs on battery",
        CheckStatus::Warn,
        "This is a fast laptop chip. It runs at full speed in short bursts, but slows down as it \
         warms up. For anything long-running, plug it into mains power and give it room to \
         breathe — it will hold a noticeably higher speed."
            .to_string(),
    ))
}

fn apple_fm_unused_check(dets: &[Detection], hw: &HardwareProfile) -> Option<CheckResult> {
    if hw.macos_major < 26 {
        return None;
    }
    let det = find(dets, "apple-fm")?;
    if det.availability == Availability::Ready {
        return None;
    }

    Some(CheckResult::new(
        "apple-fm-unused",
        "A free option already on this Mac",
        CheckStatus::Warn,
        "This Mac runs a version of macOS that includes Apple's own AI model, but it is not \
         available for use. It costs nothing, needs no downloads, and never sends anything off \
         the machine — worth turning on."
            .to_string(),
    ))
}

// --------------------------------------------------------------- helpers ---

fn find<'a>(dets: &'a [Detection], id: &str) -> Option<&'a Detection> {
    dets.iter().find(|d| d.id == id)
}

fn detail<'a>(det: &'a Detection, key: &str) -> Option<&'a str> {
    det.details
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The explanation half of a detection's friendly line, for reuse in prose.
fn short(det: &Detection) -> &str {
    det.friendly
        .split_once(" — ")
        .map(|(_, rest)| rest)
        .unwrap_or(&det.friendly)
}

/// `"22.2B"` as a number of billions.
fn size_in_billions(raw: &str) -> Option<f32> {
    raw.strip_suffix(['B', 'b'])?.parse().ok()
}

/// Free gigabytes from `df -g /` output.
fn free_gb(out: &str) -> Option<u32> {
    // Filesystem  1G-blocks  Used  Available  Capacity ...
    out.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;
    use crate::sys::testing::MockSys;

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

    fn detection(id: &'static str, availability: Availability) -> Detection {
        Detection {
            id,
            name: id,
            category: Category::LocalRuntime,
            availability,
            version: None,
            details: Vec::new(),
            friendly: format!("{id} — a thing that runs models"),
        }
    }

    fn with_detail(mut det: Detection, key: &str, value: &str) -> Detection {
        det.details.push((key.to_string(), value.to_string()));
        det
    }

    /// A Mac with Homebrew already installed, so unrelated checks stay quiet.
    fn brewed() -> MockSys {
        MockSys::new().with_cmd("brew --version", "Homebrew 6.0.15")
    }

    fn check<'a>(checks: &'a [CheckResult], id: &str) -> &'a CheckResult {
        checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no check with id {id}"))
    }

    #[test]
    fn missing_ollama_yields_fail_with_install_guide() {
        let dets = vec![detection("ollama", Availability::NotFound)];
        let r = health_checks(&dets, &hw(ChipTier::Base, 16, true), &brewed());
        let c = check(&r, "ollama-installed");

        assert_eq!(c.status, CheckStatus::Fail);
        let fix = c.fix.as_ref().expect("a failure should come with a fix");
        assert!(
            fix.commands
                .iter()
                .any(|c| c.contains("brew install ollama"))
        );
        assert!(!fix.steps.is_empty(), "human steps, not just commands");
    }

    #[test]
    fn stopped_ollama_yields_warn_with_start_command() {
        let dets = vec![detection("ollama", Availability::InstalledNotRunning)];
        let r = health_checks(&dets, &hw(ChipTier::Base, 16, true), &brewed());
        let c = check(&r, "ollama-installed");

        assert_eq!(c.status, CheckStatus::Warn);
        let fix = c.fix.as_ref().expect("a warning should come with a fix");
        assert!(
            fix.commands
                .iter()
                .any(|c| c.contains("ollama serve") || c.contains("brew services start ollama"))
        );
    }

    #[test]
    fn low_ram_perf_check_recommends_smaller_quant() {
        let r = performance_checks(&[], &hw(ChipTier::Base, 8, true), &brewed());

        assert!(r.iter().any(|c| c.explanation.contains("4-bit")));
    }

    /// Homebrew is the prerequisite for nearly every other fix, so it has to
    /// be the first thing the user is told about.
    #[test]
    fn homebrew_is_checked_before_anything_that_needs_it() {
        let dets = vec![detection("ollama", Availability::NotFound)];
        let r = health_checks(&dets, &hw(ChipTier::Base, 16, true), &MockSys::new());

        assert_eq!(r[0].id, "homebrew-installed");
        assert_eq!(r[0].status, CheckStatus::Fail);
        assert!(r[0].fix.as_ref().is_some_and(|f| !f.commands.is_empty()));
    }

    /// The engine returns guides; it must never run them itself.
    #[test]
    fn no_check_ever_executes_anything() {
        let dets = vec![
            detection("ollama", Availability::NotFound),
            detection("mlx", Availability::Partial("missing bits".to_string())),
        ];
        let hw = hw(ChipTier::Max, 8, true);
        let all: Vec<_> = health_checks(&dets, &hw, &MockSys::new())
            .into_iter()
            .chain(performance_checks(&dets, &hw, &MockSys::new()))
            .collect();

        // Every command is inert text for the user to copy, and every guide
        // that offers commands also explains them in words.
        for c in &all {
            if let Some(fix) = &c.fix {
                assert!(
                    fix.commands.is_empty() || !fix.steps.is_empty(),
                    "{}: commands without plain-English steps",
                    c.id
                );
            }
        }
    }

    /// The size comes from the probe, because the model's name usually cannot
    /// supply it.
    #[test]
    fn an_oversized_installed_model_is_flagged() {
        let ollama = with_detail(
            with_detail(
                detection("ollama", Availability::Ready),
                "Models installed",
                "codestral:latest",
            ),
            "Largest model",
            "70.0B",
        );
        let r = performance_checks(&[ollama], &hw(ChipTier::Base, 16, true), &brewed());
        let c = check(&r, "ram-headroom");

        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.explanation.contains("70.0B"));
    }

    #[test]
    fn a_model_that_fits_is_not_flagged() {
        let ollama = with_detail(
            with_detail(
                detection("ollama", Availability::Ready),
                "Models installed",
                "qwen3:4b",
            ),
            "Largest model",
            "4.0B",
        );
        let r = performance_checks(&[ollama], &hw(ChipTier::Pro, 48, true), &brewed());

        assert_eq!(check(&r, "ram-headroom").status, CheckStatus::Pass);
    }

    #[test]
    fn running_ollama_without_models_is_flagged() {
        let ollama = with_detail(
            detection("ollama", Availability::Ready),
            "Models installed",
            "none yet",
        );
        let r = health_checks(&[ollama], &hw(ChipTier::Base, 16, true), &brewed());
        let c = check(&r, "ollama-has-models");

        assert_eq!(c.status, CheckStatus::Warn);
        assert!(
            c.fix
                .as_ref()
                .is_some_and(|f| f.commands.iter().any(|c| c.contains("ollama pull")))
        );
    }

    #[test]
    fn low_disk_space_is_flagged_from_real_df_output() {
        let df = "Filesystem     1G-blocks Used Available Capacity iused      ifree %iused  \
                  Mounted on\n/dev/disk3s1s1       926   11         8     4%  481133 3062898880  \
                  0%   /";
        let sys = MockSys::new()
            .with_cmd("brew --version", "Homebrew 6.0.15")
            .with_cmd("df -g /", df);
        let r = performance_checks(&[], &hw(ChipTier::Base, 16, true), &sys);
        let c = check(&r, "disk-space");

        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.explanation.contains('8'));
    }

    #[test]
    fn ample_disk_space_passes() {
        let df = "Filesystem     1G-blocks Used Available Capacity iused      ifree %iused  \
                  Mounted on\n/dev/disk3s1s1       926   11       292     4%  481133 3062898880  \
                  0%   /";
        let sys = MockSys::new()
            .with_cmd("brew --version", "Homebrew 6.0.15")
            .with_cmd("df -g /", df);
        let r = performance_checks(&[], &hw(ChipTier::Base, 16, true), &sys);

        assert_eq!(check(&r, "disk-space").status, CheckStatus::Pass);
    }

    /// An unreadable disk must not produce a scary warning.
    #[test]
    fn unreadable_disk_space_does_not_alarm_the_user() {
        let r = performance_checks(&[], &hw(ChipTier::Base, 16, true), &brewed());

        assert_eq!(check(&r, "disk-space").status, CheckStatus::Pass);
    }

    #[test]
    fn mlx_is_suggested_over_llamacpp_on_apple_silicon() {
        let dets = vec![
            detection("llamacpp", Availability::Ready),
            detection("mlx", Availability::NotFound),
        ];
        let r = performance_checks(&dets, &hw(ChipTier::Pro, 32, true), &brewed());

        assert_eq!(check(&r, "mlx-preferred").status, CheckStatus::Warn);
    }

    #[test]
    fn mlx_is_not_suggested_when_already_installed_or_on_intel() {
        let both_ready = vec![
            detection("llamacpp", Availability::Ready),
            detection("mlx", Availability::Ready),
        ];
        let r = performance_checks(&both_ready, &hw(ChipTier::Pro, 32, true), &brewed());
        assert!(r.iter().all(|c| c.id != "mlx-preferred"));

        let intel = vec![
            detection("llamacpp", Availability::Ready),
            detection("mlx", Availability::NotFound),
        ];
        let r = performance_checks(&intel, &hw(ChipTier::Intel, 32, true), &brewed());
        assert!(r.iter().all(|c| c.id != "mlx-preferred"));
    }

    #[test]
    fn fast_laptops_get_a_thermal_note_and_desktops_do_not() {
        let laptop = performance_checks(&[], &hw(ChipTier::Max, 64, true), &brewed());
        assert_eq!(check(&laptop, "laptop-thermals").status, CheckStatus::Warn);

        let desktop = performance_checks(&[], &hw(ChipTier::Max, 64, false), &brewed());
        assert!(desktop.iter().all(|c| c.id != "laptop-thermals"));

        let modest = performance_checks(&[], &hw(ChipTier::Base, 16, true), &brewed());
        assert!(modest.iter().all(|c| c.id != "laptop-thermals"));
    }

    /// Not knowing whether Apple Intelligence is on is worth one nudge — but
    /// only when the model is actually there to be used.
    #[test]
    fn unknown_apple_intelligence_is_raised_only_when_the_model_is_ready() {
        let ready = with_detail(
            detection("apple-fm", Availability::Ready),
            "Apple Intelligence",
            "Unknown — check System Settings ▸ Apple Intelligence & Siri",
        );
        let r = health_checks(&[ready], &hw(ChipTier::Pro, 32, true), &brewed());
        assert_eq!(check(&r, "apple-intelligence-on").status, CheckStatus::Warn);

        let absent = vec![detection("apple-fm", Availability::NotFound)];
        let r = health_checks(&absent, &hw(ChipTier::Pro, 32, true), &brewed());
        assert!(r.iter().all(|c| c.id != "apple-intelligence-on"));
    }

    /// A cloud service nobody uses is not a fault, so it must not be reported
    /// as one. Six red crosses for services the user never wanted would bury
    /// the findings that matter.
    fn cloud(id: &'static str, availability: Availability) -> Detection {
        let mut d = detection(id, availability);
        d.category = Category::CloudProvider;
        d
    }

    #[test]
    fn unused_cloud_services_are_not_reported_as_failures() {
        let dets = vec![
            cloud("claude", Availability::NotFound),
            cloud("groq", Availability::NotFound),
        ];
        let r = health_checks(&dets, &hw(ChipTier::Pro, 32, true), &brewed());

        assert!(
            r.iter()
                .all(|c| c.status != CheckStatus::Fail || c.id == "homebrew-installed"),
            "an unused cloud service is not a failure"
        );
        // But the user is told once that no cloud option exists at all.
        assert_eq!(check(&r, "cloud-access").status, CheckStatus::Warn);
    }

    #[test]
    fn a_configured_cloud_service_passes() {
        let dets = vec![
            cloud("claude", Availability::Ready),
            cloud("groq", Availability::NotFound),
        ];
        let r = health_checks(&dets, &hw(ChipTier::Pro, 32, true), &brewed());

        assert_eq!(check(&r, "cloud-access").status, CheckStatus::Pass);
    }

    /// Every finding has to be readable by someone who has never heard of any
    /// of this, and anything actionable has to say what to do.
    #[test]
    fn every_check_explains_itself_in_plain_english() {
        let dets = vec![
            detection("ollama", Availability::NotFound),
            detection("lmstudio", Availability::InstalledNotRunning),
            detection("llamacpp", Availability::Ready),
            detection("mlx", Availability::Partial("mlx-lm missing".to_string())),
            detection("apple-fm", Availability::NotFound),
            cloud("claude", Availability::Partial("no key".to_string())),
        ];
        let hw = hw(ChipTier::Max, 8, true);
        let all: Vec<_> = health_checks(&dets, &hw, &MockSys::new())
            .into_iter()
            .chain(performance_checks(&dets, &hw, &MockSys::new()))
            .collect();

        assert!(all.len() > 8, "expected a full sweep, got {}", all.len());
        for c in &all {
            assert!(!c.title.is_empty(), "{}: no title", c.id);
            assert!(
                c.explanation.len() > 30,
                "{}: explanation too terse: {}",
                c.id,
                c.explanation
            );
            assert!(
                c.explanation.ends_with('.') || c.explanation.ends_with('!'),
                "{}: explanation should read as a sentence",
                c.id
            );
        }
    }

    /// Two half-configured services must not share a check id — the wizard
    /// addresses findings by id, so a collision would lose one of them.
    #[test]
    fn several_half_configured_cloud_services_keep_distinct_ids() {
        let dets = vec![
            cloud("claude", Availability::Partial("no key".to_string())),
            cloud("openai", Availability::Partial("no key".to_string())),
            cloud("groq", Availability::Partial("no key".to_string())),
        ];
        let r = health_checks(&dets, &hw(ChipTier::Pro, 32, true), &brewed());

        let mut ids: Vec<_> = r.iter().map(|c| c.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate check id across providers");
        assert_eq!(
            r.iter().filter(|c| c.id.ends_with("-configured")).count(),
            3
        );
    }

    /// Check ids are how the TUI wizard addresses individual findings.
    #[test]
    fn health_check_ids_are_unique() {
        let dets = vec![
            detection("ollama", Availability::Ready),
            detection("lmstudio", Availability::Ready),
            detection("llamacpp", Availability::Ready),
            detection("mlx", Availability::Ready),
            detection("apple-fm", Availability::Ready),
        ];
        let r = health_checks(&dets, &hw(ChipTier::Pro, 32, true), &brewed());

        let mut ids: Vec<_> = r.iter().map(|c| c.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate check id");
    }
}
