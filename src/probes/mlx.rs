//! MLX — Apple's own machine-learning framework for Apple Silicon.
//!
//! MLX comes in two pieces that install separately: the core library, and the
//! `mlx-lm` toolkit that actually runs language models. Having the first
//! without the second is the common half-configured case, so it gets its own
//! state rather than being reported as ready.

use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

/// Asks Python whether the core library is importable, and for its version.
const CORE_SCRIPT: &str = "import mlx.core; print(mlx.core.__version__)";

/// The same question for the language-model toolkit, used when its command-line
/// helper is not on PATH.
const TOOLKIT_SCRIPT: &str = "import mlx_lm; print(mlx_lm.__version__)";

pub struct MlxProbe;

impl Probe for MlxProbe {
    fn id(&self) -> &'static str {
        "mlx"
    }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        let core_version = sys
            .run("python3", &["-c", CORE_SCRIPT])
            .filter(|v| !v.is_empty());

        // The console script is the usual signal; the import is a fallback for
        // installs whose scripts directory is not on PATH.
        let toolkit = sys.run("mlx_lm.generate", &["--help"]).is_some()
            || sys.run("python3", &["-c", TOOLKIT_SCRIPT]).is_some();

        let mut details = Vec::new();
        if core_version.is_some() {
            details.push((
                "Model toolkit (mlx-lm)".to_string(),
                if toolkit {
                    "installed".to_string()
                } else {
                    "not installed".to_string()
                },
            ));
        }

        // The core library is what makes MLX present at all; without it there
        // is nothing here, whatever else Python may have lying around.
        let availability = match (core_version.is_some(), toolkit) {
            (true, true) => Availability::Ready,
            (true, false) => {
                Availability::Partial("Python stack present, mlx-lm missing".to_string())
            }
            (false, _) => Availability::NotFound,
        };

        Detection {
            id: "mlx",
            name: "MLX",
            category: Category::OptimisedRuntime,
            availability,
            version: core_version,
            details,
            friendly: "MLX — Apple's own toolkit that makes AI models run fastest on Apple Silicon"
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::testing::MockSys;

    /// The core-library check, spelled the way `MockSys` keys commands.
    const CORE_CMD: &str = "python3 -c import mlx.core; print(mlx.core.__version__)";
    const TOOLKIT_CMD: &str = "python3 -c import mlx_lm; print(mlx_lm.__version__)";

    #[test]
    fn mlx_fully_installed_is_ready() {
        let sys = MockSys::new()
            .with_cmd(CORE_CMD, "0.32.0")
            .with_cmd("mlx_lm.generate --help", "usage: mlx_lm.generate ...");
        let d = MlxProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
        assert_eq!(d.version.as_deref(), Some("0.32.0"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Model toolkit (mlx-lm)" && v == "installed")
        );
    }

    /// The common real-world case: MLX arrives as a dependency of something
    /// else, so the core library is present but the toolkit never was.
    #[test]
    fn mlx_core_without_toolkit_is_partial() {
        let sys = MockSys::new().with_cmd(CORE_CMD, "0.32.0");
        let d = MlxProbe.detect(&sys);

        assert_eq!(
            d.availability,
            Availability::Partial("Python stack present, mlx-lm missing".to_string())
        );
        assert_eq!(d.version.as_deref(), Some("0.32.0"));
        assert!(
            d.details
                .iter()
                .any(|(k, v)| k == "Model toolkit (mlx-lm)" && v == "not installed")
        );
    }

    #[test]
    fn mlx_absent() {
        let d = MlxProbe.detect(&MockSys::new());

        assert_eq!(d.availability, Availability::NotFound);
        assert!(d.version.is_none());
        assert!(d.details.is_empty());
    }

    /// An install into a virtual environment can leave the module importable
    /// while its command-line helper is not on PATH.
    #[test]
    fn toolkit_is_found_through_python_when_the_command_is_not_on_path() {
        let sys = MockSys::new()
            .with_cmd(CORE_CMD, "0.32.0")
            .with_cmd(TOOLKIT_CMD, "0.28.4");
        let d = MlxProbe.detect(&sys);

        assert_eq!(d.availability, Availability::Ready);
    }

    /// Without the core library there is no MLX, whatever else is installed.
    #[test]
    fn toolkit_without_the_core_library_is_not_found() {
        let sys = MockSys::new().with_cmd("mlx_lm.generate --help", "usage: ...");
        let d = MlxProbe.detect(&sys);

        assert_eq!(d.availability, Availability::NotFound);
    }

    /// The half-configured state has to explain itself: a reader seeing
    /// "Partial" must learn what is missing without looking anything up.
    #[test]
    fn partial_state_names_the_missing_piece() {
        let sys = MockSys::new().with_cmd(CORE_CMD, "0.32.0");
        let Availability::Partial(reason) = MlxProbe.detect(&sys).availability else {
            panic!("expected the half-configured state");
        };
        assert!(reason.contains("mlx-lm"));
    }

    #[test]
    fn carries_a_plain_english_explanation() {
        let d = MlxProbe.detect(&MockSys::new());
        assert!(d.friendly.starts_with("MLX — "));
        assert!(d.friendly.len() > 30);
    }
}
