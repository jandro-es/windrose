# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Detection** of eleven AI options across four groups: Ollama and LM Studio
  (runs on your Mac), llama.cpp and MLX (speed-optimised engines), Apple
  Foundation Models (built into macOS 26 and later), and Claude, OpenAI, Gemini,
  Perplexity, Mistral and Groq (cloud services). Probes run concurrently and
  report in a stable order.
- **Scoring** of how well this Mac runs models on its own: separate memory and
  chip scores combined so the weaker one dominates, plus a verdict and a speed
  estimate for five model sizes. Documented in `docs/SCORING.md`.
- **Doctor**, with health checks (is anything missing or half-finished?) and
  performance checks (could this Mac do better?). Every finding carries numbered
  plain-English steps as well as copy-pasteable commands.
- **Interactive dashboard** with five tabs, a permanent "What is this?" pane, a
  detail popup, a glossary behind `?`, and a doctor wizard that copies commands
  to the clipboard.
- **Commands**: `scan`, `score`, `doctor`, `report --format md|json`, and the
  dashboard by default. `--json` everywhere for scripting.
- **Manual pages** for the root command and each subcommand, generated from the
  command-line definitions and checked for drift in CI.
- **Distribution** via cargo-dist: universal binaries, a Homebrew formula, and a
  curl installer. See `docs/RELEASING.md`.
- Crate scaffold pinned to Rust 1.85 (edition 2024) with the project dependency
  set, and CI on macOS checking formatting, Clippy (warnings denied), tests, and
  manual-page drift.

### Notes

- Windrose never installs or changes anything: `doctor` produces guides, and the
  user runs them.
- Credential detection is presence-only. A key's value is never read, stored,
  logged or displayed.
- The only network requests are to services already running on `localhost`.
