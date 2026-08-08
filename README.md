# Windrose

**Find out what AI your Mac can already do — and what it would be good at.**

Windrose looks at your Mac and tells you which AI tools you already have, which
are ready to use, and how well the machine can run AI models on its own. It
sends nothing to the internet, installs nothing, and explains every term it
uses.

If you have never heard of any of this, that is fine. Windrose is written for
you.

```
┌ Windrose ────────────────────────────────────────────────────────────────────────┐
│ 1 Overview │ 2 On this Mac │ 3 Cloud │ 4 Score │ 5 Doctor                        │
└──────────────────────────────────────────────────────────────────────────────────┘
┌ On this Mac ─────────────────────────────────────────────────────────────────────┐
│       Option                   Version      Status                               │
│›  ✅   Ollama                   0.32.1       ready to use                         │
│   ⚠️   LM Studio                0.4.20       installed, but not running           │
│   ❌   llama.cpp                —            not installed                        │
│   ✅   Apple Foundation Models  27.0         ready to use                         │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
┌ What is this? ───────────────────────────────────────────────────────────────────┐
│Ollama — a free app that runs AI models privately on your Mac                     │
│                                                                                  │
│Press Enter for everything Windrose found about it.                               │
└──────────────────────────────────────────────────────────────────────────────────┘
 ↑↓ move · Tab switch · ? help · q quit
```

## Install

With [Homebrew](https://brew.sh) — a free tool that installs software from the
Terminal:

```bash
brew install jandro-es/tap/windrose
```

Or download it directly:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/jandro-es/windrose/releases/latest/download/windrose-installer.sh | sh
```

Or, if you already have Rust:

```bash
cargo install --git https://github.com/jandro-es/windrose
```

## Quick start

```bash
windrose            # open the dashboard — start here
windrose doctor     # what is worth fixing, and how to fix it
```

Everything else:

```bash
windrose scan                          # list every AI option found
windrose scan --json                   # the same, as data for scripts
windrose score                         # how well this Mac runs models
windrose report --format md > report.md
```

`man windrose` has the full manual, and `?` inside the dashboard shows the keys
and a glossary.

## What it looks for

| Group | What is in it |
|---|---|
| **Runs on your Mac** | Ollama, LM Studio |
| **Speed-optimised engines** | llama.cpp, MLX |
| **Built into macOS** | Apple Foundation Models (macOS 26 and later) |
| **Cloud services** | Claude, OpenAI, Gemini, Perplexity, Mistral, Groq |

## No jargon

The words you are most likely to meet. The same list is inside the dashboard
under `?`.

| Term | What it means |
|---|---|
| **model** | The AI itself — a large file trained to answer questions. Bigger models generally give better answers and run more slowly. |
| **runtime** | The program that runs a model, such as Ollama. The model is the file; the runtime is what opens it. |
| **parameters** | A rough measure of a model's size, in billions, written like "8B". More usually means better answers and more memory needed. |
| **quantisation** | Compressing a model so it takes less memory. "Q4" means about a quarter of the original size — the normal way to run models on a Mac. |
| **token** | A chunk of text a model reads and writes, a little shorter than a word. Around 30 tokens per second is comfortable reading pace. |
| **context window** | How much of the conversation a model can keep in mind at once. Longer chats push the earliest part out. |
| **on-device** | Running on your own Mac rather than a company's servers. Nothing you type leaves the machine, and it works offline. |
| **API key** | A password letting a program use a paid online service. Windrose only checks whether one exists — never reads or displays it. |

## What it will not do

- **It never installs or changes anything.** `doctor` shows you the commands and
  explains what they do. Running them is your decision.
- **It never reads your API keys.** It checks whether one exists and reports
  "yes" or "no". The value is never read, stored, logged or displayed.
- **It sends nothing anywhere.** The only network requests are to services
  already running on your own machine, on `localhost`.

## How the score works

Two numbers — available memory and chip speed — combined so the weaker one
counts for more, because the weaker one is what you actually feel. A fast chip
with 8 GB cannot run a large model at all, and 128 GB does not speed up a slow
chip.

[docs/SCORING.md](docs/SCORING.md) has the exact formula, both tables, and where
the speed estimates come from.

## Troubleshooting

**"Ollama is installed but not running."**
Start it with `ollama serve`, or `brew services start ollama` to have it start
by itself from now on.

**A tool I have installed shows as not installed.**
Windrose looks for commands on your `PATH`. If you installed something into a
shell-specific location, check `which <command>` works in a plain Terminal
window. For apps, it looks in `/Applications`.

**A cloud service shows "half set up".**
It found the app or the command but no sign-in and no API key, or the reverse.
Signing in to the app is usually enough — most services do not need a key at
all.

**`man windrose` says "No manual entry".**
The Homebrew formula installs the manual page, so this should not happen. If it
does, `windrose --help` covers the same ground, and the page is in the release
tarball if you want to install it yourself.

**The dashboard looks wrong, or my terminal is broken after a crash.**
Windrose restores the terminal on exit, on error, and on panic. If something
still looks off, `reset` in the Terminal fixes it. Please report it.

## Contributing

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains the layout, the one rule
that matters (`SysCtx`), and has a four-step recipe for adding a new probe.

```bash
cargo test                      # 185 tests, all mocked — no network, no subprocesses
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Two constraints are not negotiable, and there are tests holding them in place:
Windrose never runs an install command, and it never reads a credential's value.

## Requirements

macOS on Apple Silicon or Intel. Intel Macs work and score lower, because
Apple's AI frameworks are built for Apple Silicon — the useful answer there is
usually a cloud service, which Windrose will tell you.

Building from source needs Rust 1.85 or later.

## Licence

MIT — see [LICENSE](LICENSE).
