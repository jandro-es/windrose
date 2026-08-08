//! The operating-system boundary.
//!
//! This is the **only** module that touches the OS. Every other module takes a
//! `&dyn SysCtx` so that tests can run against [`testing::MockSys`] with no live
//! network and no real subprocesses.
//!
//! Secrets rule: [`SysCtx::env_is_set`] reports *presence* only. A credential's
//! value is never returned, stored, or logged anywhere in Windrose.

use std::path::PathBuf;
use std::time::Duration;

/// How long a probe subprocess may run before it is killed.
///
/// Probes shell out to small, fast tools (`sysctl`, `sw_vers`, `--version`
/// flags). Anything slower than this is hung, and a hung probe must never hang
/// the scan.
const CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the run loop checks whether the child has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Everything Windrose is allowed to ask the operating system.
pub trait SysCtx {
    /// Run a command, returning trimmed stdout when it exits successfully.
    ///
    /// Returns `None` if the binary is missing, exits non-zero, or times out —
    /// all of which simply mean "this option is not available here".
    fn run(&self, cmd: &str, args: &[&str]) -> Option<String>;

    /// GET a URL, returning the body. Used for localhost daemon probes.
    fn http_get(&self, url: &str, timeout_ms: u64) -> Option<String>;

    /// Whether an environment variable is set. Never exposes the value.
    fn env_is_set(&self, var: &str) -> bool;

    /// Whether a path exists on disk.
    fn path_exists(&self, path: &str) -> bool;

    /// The current user's home directory.
    fn home(&self) -> PathBuf;

    /// Put text on the clipboard, reporting whether it worked.
    ///
    /// This cannot go through [`SysCtx::run`]: `pbcopy` takes its input on
    /// stdin, and `run` deliberately gives children a null stdin. Calling
    /// `run("pbcopy", …)` does not copy anything — it silently replaces the
    /// clipboard with nothing, throwing away whatever the user had there.
    fn copy_to_clipboard(&self, text: &str) -> bool;
}

/// The production implementation, talking to the real machine.
pub struct RealSys;

impl SysCtx for RealSys {
    fn run(&self, cmd: &str, args: &[&str]) -> Option<String> {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        // Drain stdout on a helper thread. A chatty child (`system_profiler
        // -json` runs to tens of kilobytes) would otherwise fill the pipe
        // buffer and block forever while the poll loop below waits for it.
        let mut pipe = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = pipe.read_to_string(&mut buf);
            buf
        });

        let deadline = Instant::now() + CMD_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(_) => return None,
            }
        };

        let stdout = reader.join().ok()?;
        status.success().then(|| stdout.trim().to_string())
    }

    fn http_get(&self, url: &str, timeout_ms: u64) -> Option<String> {
        ureq::get(url)
            .timeout(Duration::from_millis(timeout_ms))
            .call()
            .ok()?
            .into_string()
            .ok()
    }

    fn env_is_set(&self, var: &str) -> bool {
        // `var_os` avoids decoding the value into a `String`; the `OsString` is
        // dropped immediately and the contents are never inspected.
        std::env::var_os(var).is_some()
    }

    fn path_exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }

    fn home(&self) -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn copy_to_clipboard(&self, text: &str) -> bool {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };

        // Take the pipe so it is dropped — and therefore closed — before the
        // wait below. `pbcopy` does not exit until its input ends.
        let written = match child.stdin.take() {
            Some(mut pipe) => pipe.write_all(text.as_bytes()).is_ok(),
            None => false,
        };

        child.wait().map(|s| s.success()).unwrap_or(false) && written
    }
}

#[cfg(test)]
pub mod testing {
    //! Test double for [`SysCtx`]. Probe tests programme the exact commands,
    //! URLs, variables and paths they expect; anything not programmed reads as
    //! "not present on this Mac", which is the case probes must handle anyway.

    use super::SysCtx;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MockSys {
        cmds: HashMap<String, String>,
        http: HashMap<String, String>,
        envs: HashSet<String>,
        paths: HashSet<String>,
        home: Option<PathBuf>,
        /// Everything asked of the machine, in order.
        ///
        /// A `Mutex` rather than a `RefCell`: probe tests hand one `MockSys`
        /// to several threads at once, which needs `Sync`.
        calls: Mutex<Vec<String>>,
    }

    impl MockSys {
        pub fn new() -> Self {
            Self::default()
        }

        /// Programme a command line (binary and arguments, space-joined) to
        /// succeed with the given stdout.
        pub fn with_cmd(mut self, cmd_line: &str, stdout: &str) -> Self {
            self.cmds.insert(cmd_line.to_string(), stdout.to_string());
            self
        }

        /// Programme a URL to return the given body.
        pub fn with_http(mut self, url: &str, body: &str) -> Self {
            self.http.insert(url.to_string(), body.to_string());
            self
        }

        /// Mark an environment variable as set. No value is ever involved.
        pub fn with_env(mut self, var: &str) -> Self {
            self.envs.insert(var.to_string());
            self
        }

        /// Mark a path as existing.
        pub fn with_path(mut self, path: &str) -> Self {
            self.paths.insert(path.to_string());
            self
        }

        /// Override the home directory (defaults to `/Users/test`).
        pub fn with_home(mut self, home: &str) -> Self {
            self.home = Some(PathBuf::from(home));
            self
        }

        /// Everything asked of the machine, in order, so a test can assert
        /// that something happened — or, just as importantly, did not.
        pub fn calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .expect("no test panics while holding this")
                .clone()
        }

        fn record(&self, call: String) {
            self.calls
                .lock()
                .expect("no test panics while holding this")
                .push(call);
        }
    }

    impl SysCtx for MockSys {
        fn run(&self, cmd: &str, args: &[&str]) -> Option<String> {
            let key = if args.is_empty() {
                cmd.to_string()
            } else {
                format!("{cmd} {}", args.join(" "))
            };
            self.record(key.clone());
            self.cmds.get(&key).cloned()
        }

        fn http_get(&self, url: &str, _timeout_ms: u64) -> Option<String> {
            self.http.get(url).cloned()
        }

        fn env_is_set(&self, var: &str) -> bool {
            self.envs.contains(var)
        }

        fn path_exists(&self, path: &str) -> bool {
            self.paths.contains(path)
        }

        fn home(&self) -> PathBuf {
            self.home
                .clone()
                .unwrap_or_else(|| PathBuf::from("/Users/test"))
        }

        fn copy_to_clipboard(&self, text: &str) -> bool {
            self.record(format!("pbcopy {text}"));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SysCtx;
    use super::testing::MockSys;

    #[test]
    fn mock_returns_programmed_cmd_output() {
        let sys = MockSys::new().with_cmd("sw_vers -productVersion", "26.1");
        assert_eq!(sys.run("sw_vers", &["-productVersion"]).unwrap(), "26.1");
        assert!(sys.run("missing", &[]).is_none());
    }

    #[test]
    fn mock_returns_programmed_http_body() {
        let sys = MockSys::new().with_http("http://localhost:11434/api/tags", r#"{"models":[]}"#);
        assert_eq!(
            sys.http_get("http://localhost:11434/api/tags", 800)
                .unwrap(),
            r#"{"models":[]}"#
        );
        assert!(
            sys.http_get("http://localhost:1234/v1/models", 800)
                .is_none()
        );
    }

    #[test]
    fn mock_reports_env_presence_only() {
        let sys = MockSys::new().with_env("ANTHROPIC_API_KEY");
        assert!(sys.env_is_set("ANTHROPIC_API_KEY"));
        assert!(!sys.env_is_set("OPENAI_API_KEY"));
    }

    #[test]
    fn mock_reports_programmed_paths() {
        let sys = MockSys::new().with_path("/Applications/LM Studio.app");
        assert!(sys.path_exists("/Applications/LM Studio.app"));
        assert!(!sys.path_exists("/Applications/Nope.app"));
    }

    /// The recorder Task 14's wizard tests rely on.
    #[test]
    fn mock_records_what_was_asked_of_the_machine() {
        let sys = MockSys::new().with_cmd("brew --version", "Homebrew 6.0.15");
        sys.run("brew", &["--version"]);
        sys.run("missing", &[]);
        sys.copy_to_clipboard("brew install ollama");

        assert_eq!(
            sys.calls(),
            ["brew --version", "missing", "pbcopy brew install ollama"]
        );
    }

    /// Failed lookups are recorded too: a test proving something was *not*
    /// asked for needs the record to be complete, not just the successes.
    #[test]
    fn mock_records_calls_that_found_nothing() {
        let sys = MockSys::new();
        sys.run("nothing-here", &["--version"]);

        assert_eq!(sys.calls(), ["nothing-here --version"]);
    }

    #[test]
    fn mock_home_defaults_and_overrides() {
        assert_eq!(MockSys::new().home().to_str().unwrap(), "/Users/test");
        let sys = MockSys::new().with_home("/Users/someone");
        assert_eq!(sys.home().to_str().unwrap(), "/Users/someone");
    }
}
