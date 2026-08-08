# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What Windrose is

A macOS terminal application (`windrose`) that discovers, scores, and reports every AI option
on the user's Mac — local runtimes, Apple platform models, and frontier cloud providers — with
a guided `doctor` mode for setup, health, and performance tuning.

The authoritative spec is `docs/Windrose — Implementation Plan.md`. It contains the task
breakdown (Tasks 1–18), the exact core type signatures, and the scoring formula. Read the
relevant task section before implementing it; do not re-derive interfaces from scratch.

## Status

Pre-implementation. Only `docs/` exists. Task 1 (`cargo init`, CLI skeleton, CI) is the entry point.

## Architecture

Single crate (no workspace). Sync Rust core — **no async runtime, no tokio**. Probes are
short-lived subprocess/HTTP calls fanned out with `std::thread::scope`.

```
src/main.rs      entry: parse CLI, dispatch to cli:: or tui::
src/cli.rs       clap definitions + non-TUI command handlers
src/sys.rs       SysCtx trait — the ONLY module that touches the OS
src/hardware.rs  HardwareProfile detection (chip, RAM, GPU, macOS version)
src/model.rs     Detection, Availability, Category
src/probes/      Probe trait + registry(); one file per provider family
src/scoring.rs   DeviceScore + ModelTierFit
src/doctor.rs    CheckResult, FixGuide, health + performance assessments
src/report.rs    text / JSON / markdown renderers over ScanResult
src/tui/         Elm architecture: mod (loop), app (Model+Msg+update), view, doctor_view, help
```

Two frontends (plain CLI and Ratatui TUI) consume one UI-agnostic core. Anything that formats
for a human belongs in `report.rs` or `tui/`, never in the core modules.

### The SysCtx rule

Everything that reads the OS goes through `trait SysCtx` (`run`, `http_get`, `env_is_set`,
`path_exists`, `home`). Every other function takes `&dyn SysCtx`. `RealSys` is the production
impl; `MockSys` (builder: `with_cmd`, `with_http`, `with_env`, `with_path`) backs every test.

**No live network and no real subprocesses in unit tests.** If a test needs the OS, the design
is wrong — route it through `SysCtx`.

### Adding a probe

Implement `Probe` (`id()`, `detect(&dyn SysCtx) -> Detection`) in `src/probes/<name>.rs`, write
the three-state tests first (Ready / InstalledNotRunning-or-Partial / NotFound), then register
it in `registry()`. `run_all` must preserve registry order — reports and tests depend on it.

## Non-negotiable constraints

- **Plain-language rule.** Every user-facing string that names a technology gets a one-line
  explanation on first appearance ("Ollama — a free app that runs AI models privately on your
  Mac"). No unexplained jargon in UI, doctor output, man pages, or docs. Every `Detection`
  carries a `friendly` one-liner; every `ModelTierFit` carries plain-English `advice`.
- **Secrets rule.** Credential detection is presence-only. Never read a key's value beyond
  `is_ok()`, never log it, never put it in `Detection.details`. Detail rows are booleans and
  paths only ("API key found in environment: yes").
- **Never execute install commands.** Doctor produces `FixGuide`s; the user copies and runs
  them. Copy-to-clipboard via `pbcopy` is fine; unattended system changes are not.
- **Setup guidance is opt-in.** The engine returns guides; frontends ask before showing install
  flows. CLI prompts default to "no" when stdout is not a TTY.
- **Never leave the terminal broken.** TUI setup installs a panic hook that restores raw mode
  and the alternate screen before unwinding.
- Rust edition 2024, MSRV 1.85. Targets `aarch64-apple-darwin` (primary) and
  `x86_64-apple-darwin` (degraded scoring, still functional).
- Pinned deps: ratatui 0.29, crossterm 0.28, clap 4.5 (derive), ureq 2, serde 1, color-eyre 0.6,
  clap_mangen 0.2.

## Workflow

TDD throughout, per task step: write the failing test → run it and confirm it fails → minimal
implementation → pass → commit. The plan spells out the failing test for most steps; use it.

Commit per task with conventional-commit messages (`feat:`, `fix:`, `docs:`, `chore:`), matching
the message given at the end of each task in the plan.

Mark plan checkboxes (`- [ ]` → `- [x]`) as steps complete.

## Commands

```bash
cargo test                          # unit tests (all mocked)
cargo clippy -- -D warnings         # CI gate — warnings are errors
cargo fmt --check                   # CI gate
cargo run -- scan                   # manual smoke test on the real Mac
cargo run -- gen-man man/           # regenerate man pages (hidden subcommand)
./scripts/generate-man.sh           # gen-man + drift check
./scripts/build.sh                  # universal binary into dist/
./scripts/release.sh patch          # test → bump → tag → push (cargo-dist takes over)
```

Scripts (once Task 16 lands) all use `#!/usr/bin/env bash`, `set -euo pipefail`, a header
comment block stating what/why/usage, and support `--help`.

## Distribution

cargo-dist drives GitHub Releases, the curl installer, and the `jandro-es/homebrew-tap` formula.
Tags pushed by `release.sh` trigger the pipeline. Don't hand-edit `.github/workflows/release.yml`
— it is generated by `dist init`/`dist generate`.
