# How Windrose is built

A single Rust crate, no workspace. Two frontends — a plain command line and a
terminal dashboard — over one core that knows nothing about either.

## The shape of it

```
src/main.rs      entry: parse arguments, dispatch to cli:: or tui::
src/cli.rs       clap definitions, the non-TUI command handlers, gen-man
src/sys.rs       SysCtx — the ONLY module that touches the operating system
src/hardware.rs  HardwareProfile: chip, memory, GPU cores, macOS version
src/model.rs     Detection, Availability, Category — the shared vocabulary
src/probes/      the Probe trait, the registry, one file per provider family
src/scoring.rs   DeviceScore and the model-size fit table
src/doctor.rs    CheckResult, FixGuide, health and performance assessments
src/report.rs    gather() plus the text, markdown and JSON renderers
src/tui/         Elm architecture: mod (loop), app (state), view, doctor_view, help
```

Data flows one way:

```
SysCtx ──> hardware::profile ──┐
       └─> probes::run_all ────┼──> ScanResult ──> report::render_* ──> stdout
                               ├──> scoring::score        └──> tui::view ──> screen
                               └──> doctor::*_checks
```

`gather()` in `report.rs` is the single place the whole pipeline is wired
together. Everything above it is UI-agnostic; anything that formats for a human
belongs in `report.rs` or `tui/`, never in the core modules.

## The one rule that matters: `SysCtx`

Everything that reads the machine goes through one trait:

```rust
pub trait SysCtx {
    fn run(&self, cmd: &str, args: &[&str]) -> Option<String>;
    fn http_get(&self, url: &str, timeout_ms: u64) -> Option<String>;
    fn env_is_set(&self, var: &str) -> bool;
    fn path_exists(&self, path: &str) -> bool;
    fn home(&self) -> PathBuf;
    fn copy_to_clipboard(&self, text: &str) -> bool;
}
```

`RealSys` talks to the machine. `MockSys` backs every test, with builders
(`with_cmd`, `with_http`, `with_env`, `with_path`, `with_home`) and a `calls()`
recorder for asserting what was — and was not — asked of the machine.

**No live network and no real subprocesses in unit tests.** All 185 tests run
against `MockSys`. If a test needs the real machine, the design is wrong: route
it through `SysCtx`.

Two details in `RealSys` that are easy to get wrong:

- `run` drains the child's stdout on a helper thread. A chatty child
  (`system_profiler -json` runs to tens of kilobytes) would otherwise fill the
  pipe buffer and deadlock against the poll loop.
- `copy_to_clipboard` exists because `pbcopy` reads *stdin*, and `run` gives
  children a null stdin. `run("pbcopy", …)` does not copy — it wipes the
  clipboard.

## Concurrency

There is no async runtime and no tokio. Probes are short subprocess and
localhost calls, so `run_all` fans them out with `std::thread::scope` and joins
them **in registry order**. Reports, snapshots and the dashboard table all index
by position, so the order of results must not depend on which probe finishes
first.

## Adding a new probe in four steps

### 1. Write the failing tests first

Three states, at minimum: ready, half-configured, and absent.

```rust
#[test]
fn newthing_running_is_ready() {
    let sys = MockSys::new()
        .with_cmd("newthing --version", "newthing 1.2.3")
        .with_http("http://localhost:9999/v1/models", r#"{"data":[]}"#);
    assert_eq!(NewThingProbe.detect(&sys).availability, Availability::Ready);
}

#[test]
fn newthing_installed_but_stopped() { /* command only ⇒ InstalledNotRunning */ }

#[test]
fn newthing_absent() { /* nothing programmed ⇒ NotFound */ }
```

**Check the real output first.** Every probe in this repo found something the
documentation did not say: `ollama --version` prints a second warning line,
`lms version` contains no version at all, and most Ollama model names carry no
size. Run the command by hand and write the test around what it actually prints.

### 2. Implement `Probe` in `src/probes/newthing.rs`

```rust
use super::Probe;
use crate::model::{Availability, Category, Detection};
use crate::sys::SysCtx;

pub struct NewThingProbe;

impl Probe for NewThingProbe {
    fn id(&self) -> &'static str { "newthing" }

    fn detect(&self, sys: &dyn SysCtx) -> Detection {
        Detection {
            id: "newthing",
            name: "NewThing",
            category: Category::LocalRuntime,
            availability: Availability::NotFound,
            version: None,
            details: Vec::new(),
            friendly: "NewThing — a one-line explanation for someone who has \
                       never heard of it"
                .to_string(),
        }
    }
}
```

Two rules apply to every `Detection`:

- **Plain language.** `friendly` explains what the thing *is*, in one sentence,
  assuming no background. It is not optional.
- **Secrets.** `details` rows are booleans and paths only. A credential's value
  must never reach a `Detection` — presence is reported as `yes`/`no`.

### 3. Register it

Add it to `registry()` in `src/probes/mod.rs`. Order is presentation order:
grouped by category, most widely used first.

### 4. Run the gate

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The registry has a uniqueness test, so a duplicate id fails immediately.

## The dashboard

Elm architecture. All state lives in `App`; every input becomes a `Msg`;
`update` is the only thing that changes state. `update` touches no terminal, so
the entire interaction model is covered by ordinary tests, and `view` is checked
through ratatui's `TestBackend`.

`tui/mod.rs` owns the terminal and one hard rule: **whatever happens, the
terminal is put back the way it was found.** Raw mode and the alternate screen
belong to the user's shell, not to us. A normal exit, an error, and a panic all
restore it — the panic hook runs before the default one, so the panic message
lands on a working screen rather than a dead alternate buffer.

## Things Windrose will not do

These are design constraints, not preferences, and there are tests holding them
in place:

- **It never runs an install command.** `doctor` produces `FixGuide`s; the user
  copies and runs them. A test asserts that pressing `c` in the wizard records a
  clipboard call and that *every* recorded call is a clipboard call.
- **It never reads a credential's value.** `env_is_set` returns a `bool` and
  uses `var_os` so the value is never even decoded into a `String`.
- **It sends nothing anywhere.** The only HTTP requests are to `localhost`.
- **No unexplained jargon.** Every `Detection` carries `friendly`, every
  `ModelTierFit` carries `advice`, and `tui/help.rs` is a glossary whose own
  tests reject explanations that lean on further jargon.
