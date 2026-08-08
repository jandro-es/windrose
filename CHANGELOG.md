# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Crate scaffold pinned to Rust 1.85 (edition 2024) with the project dependency set.
- `windrose` command-line skeleton: `scan`, `score`, `doctor`, `report`, `tui`,
  plus a hidden `gen-man`. Running with no subcommand opens the dashboard.
- Global `--json` and `--no-color` flags.
- CI on macOS checking formatting, Clippy (warnings denied), and tests.
