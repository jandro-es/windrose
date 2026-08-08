//! Turning a scan into something a person can read.
//!
//! This is the only place in the core that formats for humans. Everything it
//! renders comes from [`ScanResult`]; nothing here reads the machine.

use crate::doctor::{self, CheckResult, CheckStatus};
use crate::hardware::{self, HardwareProfile};
use crate::model::{Availability, Category, Detection};
use crate::probes;
use crate::scoring::{self, DeviceScore, Fit, ModelTierFit, QUANTISATION_NOTE};
use crate::sys::SysCtx;

/// Everything one run of Windrose found out.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanResult {
    pub hardware: HardwareProfile,
    pub detections: Vec<Detection>,
    pub score: DeviceScore,
    pub health: Vec<CheckResult>,
    pub perf: Vec<CheckResult>,
}

/// The four categories in presentation order, with the headings the plain-
/// language rule requires — a reader should not need to know what a "runtime"
/// is to read the report.
const CATEGORY_HEADINGS: [(Category, &str); 4] = [
    (Category::LocalRuntime, "Runs on your Mac"),
    (Category::OptimisedRuntime, "Speed-optimised engines"),
    (Category::ApplePlatform, "Built into macOS"),
    (Category::CloudProvider, "Cloud services"),
];

/// Look at this Mac and work out everything Windrose knows how to work out.
pub fn gather(sys: &(dyn SysCtx + Sync)) -> ScanResult {
    let hardware = hardware::profile(sys);
    let detections = probes::run_all(sys);
    let score = scoring::score(&hardware);
    let health = doctor::health_checks(&detections, &hardware, sys);
    let perf = doctor::performance_checks(&detections, &hardware, sys);

    ScanResult {
        hardware,
        detections,
        score,
        health,
        perf,
    }
}

/// Marker shown against each option. Words follow it everywhere, so the report
/// still reads correctly without colour or emoji support.
fn marker(availability: &Availability) -> &'static str {
    match availability {
        Availability::Ready => "✅",
        Availability::InstalledNotRunning | Availability::Partial(_) => "⚠️",
        Availability::NotFound => "❌",
    }
}

/// Plain words for each state, for readers who cannot see the marker.
fn state_words(availability: &Availability) -> String {
    match availability {
        Availability::Ready => "ready to use".to_string(),
        Availability::InstalledNotRunning => "installed, but not running".to_string(),
        Availability::Partial(reason) => format!("half set up — {reason}"),
        Availability::NotFound => "not installed".to_string(),
    }
}

fn status_marker(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "✅",
        CheckStatus::Warn => "⚠️",
        CheckStatus::Fail => "❌",
    }
}

fn fit_words(fits: Fit) -> &'static str {
    match fits {
        Fit::Great => "Great",
        Fit::Ok => "OK",
        Fit::Tight => "Tight",
        Fit::No => "Won't fit",
    }
}

fn speed_words(tier: &ModelTierFit) -> String {
    match tier.est_tok_s {
        Some((low, high)) => format!("{low}–{high} words/sec"),
        None => "—".to_string(),
    }
}

/// One line describing the machine itself.
fn machine_line(hw: &HardwareProfile) -> String {
    let shape = if hw.is_laptop { "laptop" } else { "desktop" };
    format!(
        "{} · {} GB memory · macOS {} · {shape}",
        hw.chip_name, hw.ram_gb, hw.macos_major
    )
}

// ------------------------------------------------------------------ text ---

/// The default output: readable in a terminal, no markup.
pub fn render_text(r: &ScanResult) -> String {
    let mut out = String::new();

    out.push_str("Windrose — AI options on this Mac\n");
    out.push_str(&format!("{}\n\n", machine_line(&r.hardware)));

    for (category, heading) in CATEGORY_HEADINGS {
        let found: Vec<&Detection> = r
            .detections
            .iter()
            .filter(|d| d.category == category)
            .collect();
        if found.is_empty() {
            continue;
        }

        out.push_str(&format!("{heading}\n"));
        for det in found {
            out.push_str(&format!(
                "  {} {}{} — {}\n",
                marker(&det.availability),
                det.name,
                det.version
                    .as_deref()
                    .map(|v| format!(" {v}"))
                    .unwrap_or_default(),
                state_words(&det.availability),
            ));
        }
        out.push('\n');
    }

    out.push_str(&score_block_text(&r.score));
    out.push_str(&checks_text("Health", &r.health));
    out.push_str(&checks_text("Performance", &r.perf));
    out
}

fn score_block_text(score: &DeviceScore) -> String {
    let mut out = String::from("How well this Mac runs models on its own\n");
    out.push_str(&format!(
        "  Overall {}/100  (memory {}/100, chip {}/100)\n\n",
        score.overall, score.memory, score.compute
    ));

    for tier in &score.tiers {
        out.push_str(&format!(
            "  {:<16} {:<10} {:<18} {}\n",
            tier.label,
            fit_words(tier.fits),
            speed_words(tier),
            tier.advice
        ));
    }
    out.push_str(&format!("\n  {QUANTISATION_NOTE}\n\n"));
    out
}

fn checks_text(title: &str, checks: &[CheckResult]) -> String {
    if checks.is_empty() {
        return String::new();
    }

    let mut out = format!("{title}\n");
    for check in checks {
        out.push_str(&format!(
            "  {} {}\n",
            status_marker(check.status),
            headline(check)
        ));
    }
    out.push('\n');
    out
}

/// `Title — explanation`, without saying the name twice.
///
/// Explanations lead with the thing's name so that they stand alone in JSON,
/// where there is no title beside them. Printed under a title as well, that
/// reads as "llama.cpp — llama.cpp — a fast engine…".
fn headline(check: &CheckResult) -> String {
    if check.explanation.starts_with(&check.title) {
        check.explanation.clone()
    } else {
        format!("{} — {}", check.title, check.explanation)
    }
}

/// Just the score, for `windrose score`.
pub fn render_score(r: &ScanResult) -> String {
    format!(
        "{}\n\n{}",
        machine_line(&r.hardware),
        score_block_text(&r.score)
    )
}

/// Just the findings, for `windrose doctor`. Never includes setup steps —
/// those are opt-in and live in [`render_fixes`].
pub fn render_doctor(r: &ScanResult) -> String {
    let mut out = format!("{}\n\n", machine_line(&r.hardware));
    out.push_str(&checks_text("Health", &r.health));
    out.push_str(&checks_text("Performance", &r.perf));

    let fixable = r
        .health
        .iter()
        .chain(&r.perf)
        .filter(|c| c.fix.is_some())
        .count();
    if fixable > 0 {
        out.push_str(&format!(
            "{fixable} of these have step-by-step instructions available.\n"
        ));
    }
    out
}

/// The setup steps, shown only when the user asks for them.
///
/// Kept separate from [`render_text`] on purpose: setup guidance is opt-in, so
/// a plain `scan` or `doctor` never prints install instructions unbidden.
pub fn render_fixes(checks: &[CheckResult]) -> String {
    let mut out = String::new();

    for check in checks.iter().filter(|c| c.fix.is_some()) {
        let fix = check.fix.as_ref().expect("filtered on is_some");
        out.push_str(&format!("\n{} — {}\n", check.title, check.explanation));

        for (n, step) in fix.steps.iter().enumerate() {
            out.push_str(&format!("  {}. {step}\n", n + 1));
        }
        if !fix.commands.is_empty() {
            out.push_str("\n  Commands to copy:\n");
            for command in &fix.commands {
                out.push_str(&format!("    {command}\n"));
            }
        }
        if let Some(url) = fix.docs_url {
            out.push_str(&format!("\n  More detail: {url}\n"));
        }
    }
    out
}

// -------------------------------------------------------------- markdown ---

/// A document to save or share.
pub fn render_markdown(r: &ScanResult) -> String {
    let mut out = String::from("# Windrose report\n\n");
    out.push_str(&format!("**This Mac:** {}\n\n", machine_line(&r.hardware)));

    for (category, heading) in CATEGORY_HEADINGS {
        let found: Vec<&Detection> = r
            .detections
            .iter()
            .filter(|d| d.category == category)
            .collect();
        if found.is_empty() {
            continue;
        }

        out.push_str(&format!("## {heading}\n\n"));
        out.push_str("| | Option | Version | Status |\n|---|---|---|---|\n");
        for det in found {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                marker(&det.availability),
                det.name,
                det.version.as_deref().unwrap_or("—"),
                state_words(&det.availability),
            ));
        }
        out.push('\n');

        // The friendly line is the whole point of the plain-language rule, so
        // it gets its own space rather than being squeezed into the table.
        for det in r.detections.iter().filter(|d| d.category == category) {
            out.push_str(&format!("- {}\n", det.friendly));
        }
        out.push('\n');
    }

    out.push_str(&score_block_markdown(&r.score));
    out.push_str(&checks_markdown("Health", &r.health));
    out.push_str(&checks_markdown("Performance", &r.perf));
    out
}

fn score_block_markdown(score: &DeviceScore) -> String {
    let mut out = String::from("## How well this Mac runs models on its own\n\n");
    out.push_str(&format!(
        "**Overall {}/100** — memory {}/100, chip {}/100.\n\n",
        score.overall, score.memory, score.compute
    ));
    out.push_str("| Model size | Fits | Rough speed | What that means |\n|---|---|---|---|\n");

    for tier in &score.tiers {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            tier.label,
            fit_words(tier.fits),
            speed_words(tier),
            tier.advice
        ));
    }
    out.push_str(&format!("\n> {QUANTISATION_NOTE}\n\n"));
    out
}

fn checks_markdown(title: &str, checks: &[CheckResult]) -> String {
    if checks.is_empty() {
        return String::new();
    }

    let mut out = format!("## {title}\n\n");
    for check in checks {
        out.push_str(&format!(
            "- {} {}\n",
            status_marker(check.status),
            headline(check)
        ));
    }
    out.push('\n');
    out
}

// ------------------------------------------------------------------ json ---

/// The same information as data. Never contains a credential — detections
/// carry presence answers only, which the probes guarantee.
pub fn render_json(r: &ScanResult) -> String {
    serde_json::to_string_pretty(r)
        .unwrap_or_else(|e| format!("{{\"error\":\"could not render report: {e}\"}}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::ChipTier;
    use crate::sys::testing::MockSys;

    fn fixture() -> ScanResult {
        let hardware = HardwareProfile {
            chip_name: "Apple M4 Pro".to_string(),
            chip_tier: ChipTier::Pro,
            ram_gb: 48,
            gpu_cores: Some(20),
            macos_major: 26,
            is_apple_silicon: true,
            is_laptop: true,
        };

        let detections = vec![
            det(
                "ollama",
                "Ollama",
                Category::LocalRuntime,
                Availability::Ready,
            ),
            det(
                "lmstudio",
                "LM Studio",
                Category::LocalRuntime,
                Availability::InstalledNotRunning,
            ),
            det(
                "llamacpp",
                "llama.cpp",
                Category::OptimisedRuntime,
                Availability::NotFound,
            ),
            det(
                "apple-fm",
                "Apple Foundation Models",
                Category::ApplePlatform,
                Availability::Ready,
            ),
            det(
                "claude",
                "Claude",
                Category::CloudProvider,
                Availability::Partial("no sign-in found".to_string()),
            ),
        ];

        let score = scoring::score(&hardware);
        let sys = MockSys::new();
        let health = doctor::health_checks(&detections, &hardware, &sys);
        let perf = doctor::performance_checks(&detections, &hardware, &sys);

        ScanResult {
            hardware,
            detections,
            score,
            health,
            perf,
        }
    }

    fn det(
        id: &'static str,
        name: &'static str,
        category: Category,
        availability: Availability,
    ) -> Detection {
        Detection {
            id,
            name,
            category,
            availability,
            version: Some("1.2.3".to_string()),
            details: vec![("Example row".to_string(), "yes".to_string())],
            friendly: format!("{name} — a plain-English explanation of what this is"),
        }
    }

    #[test]
    fn markdown_contains_every_category_heading() {
        let md = render_markdown(&fixture());

        for (_, heading) in CATEGORY_HEADINGS {
            assert!(md.contains(heading), "missing heading: {heading}");
        }
    }

    #[test]
    fn markdown_marks_each_availability_state() {
        let md = render_markdown(&fixture());

        assert!(md.contains("✅"), "no ready marker");
        assert!(md.contains("⚠️"), "no partial marker");
        assert!(md.contains("❌"), "no missing marker");
    }

    #[test]
    fn markdown_contains_the_score_block() {
        let md = render_markdown(&fixture());
        let score = scoring::score(&fixture().hardware);

        assert!(md.contains(&format!("**Overall {}/100**", score.overall)));
        assert!(md.contains("30B class (Q4)"));
        assert!(md.contains("70B class (Q4)"));
    }

    #[test]
    fn json_round_trips() {
        let json = render_json(&fixture());
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("render_json must emit valid JSON");

        assert_eq!(parsed["hardware"]["ram_gb"], 48);
        assert!(
            parsed["detections"]
                .as_array()
                .is_some_and(|d| d.len() == 5)
        );
        assert!(parsed["score"]["overall"].is_number());
        assert!(parsed["health"].as_array().is_some());
        assert!(parsed["perf"].as_array().is_some());
    }

    /// A marker on its own is not an explanation. Anyone reading without
    /// colour, emoji, or sight of the symbols still needs the answer.
    #[test]
    fn every_state_is_also_spelled_out_in_words() {
        let text = render_text(&fixture());

        assert!(text.contains("ready to use"));
        assert!(text.contains("installed, but not running"));
        assert!(text.contains("not installed"));
        assert!(text.contains("half set up — no sign-in found"));
    }

    /// Setup guidance is opt-in: a plain scan must never print install steps.
    #[test]
    fn plain_output_never_shows_install_commands() {
        let r = fixture();

        for rendered in [render_text(&r), render_markdown(&r)] {
            assert!(
                !rendered.contains("brew install"),
                "setup commands must only appear when the user asks for them"
            );
        }
    }

    #[test]
    fn fixes_are_available_on_request_with_steps_and_commands() {
        let r = fixture();
        let fixes = render_fixes(&r.health);

        assert!(fixes.contains("brew install"), "commands should be offered");
        assert!(fixes.contains("1."), "numbered plain-English steps");
    }

    /// The jargon in the tier labels is explained wherever those labels appear.
    #[test]
    fn the_quantisation_note_travels_with_the_tier_table() {
        let r = fixture();

        assert!(render_text(&r).contains("compressed"));
        assert!(render_markdown(&r).contains("compressed"));
    }

    /// Categories with nothing in them would render as empty tables.
    #[test]
    fn empty_categories_are_omitted() {
        let mut r = fixture();
        r.detections
            .retain(|d| d.category == Category::LocalRuntime);
        let md = render_markdown(&r);

        // Match the heading itself: "Cloud services" also appears as a health
        // check title, which is a different thing entirely.
        assert!(md.contains("## Runs on your Mac"));
        assert!(!md.contains("## Cloud services"));
    }

    /// `gather` is the one place the whole pipeline is wired together.
    #[test]
    fn gather_assembles_a_complete_result_from_a_mocked_machine() {
        let sys = MockSys::new()
            .with_cmd("sysctl -n machdep.cpu.brand_string", "Apple M4 Pro")
            .with_cmd("sysctl -n hw.memsize", "51539607552")
            .with_cmd("sw_vers -productVersion", "26.1")
            .with_cmd("ollama --version", "ollama version is 0.31.0");

        let r = gather(&sys);

        assert_eq!(r.hardware.ram_gb, 48);
        assert_eq!(r.hardware.chip_tier, ChipTier::Pro);
        assert_eq!(r.detections.len(), probes::registry().len());
        assert!(r.score.overall > 0);
        assert!(!r.health.is_empty());
        assert!(!r.perf.is_empty());
    }

    /// Explanations lead with the thing's name so they stand alone as data.
    /// Printed under a title, that would say the name twice.
    #[test]
    fn a_check_never_says_its_own_name_twice() {
        let dets = vec![det(
            "llamacpp",
            "llama.cpp",
            Category::OptimisedRuntime,
            Availability::NotFound,
        )];
        let hw = fixture().hardware;
        let health = doctor::health_checks(&dets, &hw, &MockSys::new());
        let r = ScanResult {
            hardware: hw.clone(),
            detections: dets,
            score: scoring::score(&hw),
            health,
            perf: Vec::new(),
        };

        for rendered in [render_text(&r), render_markdown(&r)] {
            assert!(
                !rendered.contains("llama.cpp — llama.cpp"),
                "the name is printed twice:\n{rendered}"
            );
        }
        // The explanation still stands on its own in the data.
        assert!(r.health.iter().any(|c| c.explanation.contains("llama.cpp")));
    }

    /// Does this text contain something shaped like an API key?
    ///
    /// Deliberately not a bare `contains("sk-")`: the check id `disk-space`
    /// contains that substring, and a test that cries wolf on its own output
    /// gets ignored. A real key is the prefix followed by a long token.
    fn looks_like_a_key(text: &str) -> bool {
        text.match_indices("sk-").any(|(at, _)| {
            text[at + 3..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .count()
                >= 8
        })
    }

    #[test]
    fn the_key_detector_knows_a_key_from_a_word() {
        assert!(looks_like_a_key("sk-ant-api03-abcdefghijklmnop"));
        assert!(looks_like_a_key("key: sk-proj-0123456789abcdef"));
        assert!(!looks_like_a_key("disk-space"));
        assert!(!looks_like_a_key("the disk-space check"));
    }

    /// The secrets rule, enforced at the last place data can escape.
    #[test]
    fn no_renderer_can_emit_a_key_shaped_value() {
        let r = gather(&MockSys::new().with_env("ANTHROPIC_API_KEY"));

        for rendered in [render_text(&r), render_markdown(&r), render_json(&r)] {
            assert!(
                !looks_like_a_key(&rendered),
                "a key value reached the output"
            );
        }
    }
}
