//! The vocabulary every probe speaks.
//!
//! One [`Detection`] describes one AI option on this Mac. Probes produce them,
//! the scoring and doctor engines read them, and the report and TUI layers
//! render them — nothing here knows or cares which frontend is running.

/// How ready an option is to actually be used, right now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Availability {
    /// Installed, running, and usable without further setup.
    Ready,
    /// Present on disk but not currently running.
    InstalledNotRunning,
    /// Half configured. Carries a plain sentence saying which half is missing.
    Partial(String),
    /// No trace of it on this Mac.
    NotFound,
}

/// The four families Windrose groups options into, each with a friendly
/// heading used by every renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Category {
    /// Apps that run AI models on this Mac — "Runs on your Mac".
    LocalRuntime,
    /// Engines tuned for maximum speed on this chip — "Speed-optimised engines".
    OptimisedRuntime,
    /// Models Apple ships as part of macOS — "Built into macOS".
    ApplePlatform,
    /// Services that run models on someone else's computer — "Cloud services".
    CloudProvider,
}

/// One AI option found (or not found) on this Mac.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Detection {
    /// Stable machine identifier, e.g. "ollama". Never shown to the user.
    pub id: &'static str,
    /// Display name, e.g. "Ollama".
    pub name: &'static str,
    pub category: Category,
    pub availability: Availability,
    pub version: Option<String>,
    /// Label/value rows shown in the detail pane, e.g.
    /// ("Models installed", "llama3.2, qwen3").
    ///
    /// Secrets rule: a value here must never contain a credential. Presence is
    /// reported as a plain "yes"/"no", never as the key itself.
    pub details: Vec<(String, String)>,
    /// One-line plain-English explanation of what this thing is, for readers
    /// who have never heard of it.
    pub friendly: String,
}
