# Windrose

**Find out what AI your Mac can already do.**

Windrose looks at your Mac and tells you which AI tools you have, which are
ready to use, and how well the machine can run AI models on its own — without
sending anything to the internet.

It explains every term it uses. If you have never heard of any of this, that is
fine: Windrose is written for you.

```
Windrose — AI options on this Mac
Apple M4 Pro · 48 GB memory · macOS 26 · laptop

Runs on your Mac
  ✅ Ollama 0.32.1 — ready to use
  ⚠️ LM Studio 0.4.20 — installed, but not running

Built into macOS
  ✅ Apple Foundation Models 26.1 — ready to use

How well this Mac runs models on its own
  Overall 80/100  (memory 95/100, chip 70/100)
```

## Install

```bash
brew install jandro/tap/windrose
```

Or download it directly:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/jandro/windrose/releases/latest/download/windrose-installer.sh | sh
```

## Use

```bash
windrose              # the interactive dashboard
windrose scan         # list every AI option found
windrose score        # how well this Mac runs models on its own
windrose doctor       # what is worth fixing, and how
windrose report --format md > report.md
```

`man windrose` has the full manual.

## What it will not do

- **It never installs or changes anything.** `doctor` shows you the commands and
  explains them. Running them is your decision.
- **It never reads your API keys.** It checks whether one exists and reports
  "yes" or "no". The value is never read, stored, logged or displayed.
- **It sends nothing anywhere.** The only network requests are to services
  already running on your own machine, on `localhost`.

## What it looks for

| | |
|---|---|
| **Runs on your Mac** | Ollama, LM Studio |
| **Speed-optimised engines** | llama.cpp, MLX |
| **Built into macOS** | Apple Foundation Models |
| **Cloud services** | Claude, OpenAI, Gemini, Perplexity, Mistral, Groq |

## How the score works

Two numbers — available memory and chip speed — combined so the weaker one
counts for more, because the weaker one is what you actually feel. See
[docs/SCORING.md](docs/SCORING.md) for the exact formula and both tables.

## Requirements

macOS on Apple Silicon or Intel. Intel Macs work, and score lower, because
Apple's AI frameworks are built for Apple Silicon.

## Licence

MIT — see [LICENSE](LICENSE).
