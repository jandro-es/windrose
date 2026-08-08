# How Windrose scores your Mac

Windrose gives your Mac three numbers out of 100 and a verdict on five model
sizes. This page explains exactly how those are worked out, so you can decide
how much to trust them.

**These are estimates, not benchmarks.** They answer "what can I sensibly run?"
rather than "how fast is this machine?". Nothing here is measured on your Mac —
it is all worked out from the chip, the memory and published community results.

## The three numbers

### Memory (0–100)

Based on total system memory, on this scale:

| Memory | Score |
|-------:|------:|
| 8 GB   | 30 |
| 16 GB  | 55 |
| 32 GB  | 75 |
| 48 GB  | 85 |
| 64 GB+ | 95 |

Anything between two rows is interpolated — 24 GB scores 65. The score stops
climbing past 64 GB, because beyond that memory has stopped being the thing
holding you back.

### Compute (0–100)

Based on the chip family, plus a small bonus for a large graphics processor:

| Chip | Base score |
|------|-----------:|
| Intel | 10 |
| Apple M-series (base) | 55 |
| Pro | 70 |
| Max | 85 |
| Ultra | 95 |

**+5** if the chip has 30 or more graphics cores.

Intel scores low because Apple's AI frameworks are built for Apple Silicon.
An Intel Mac still works — it just leans on cloud services instead.

### Overall (0–100)

```
overall = (weaker of memory/compute × 0.6) + (stronger × 0.4)
          − 10 if this is a Max or Ultra laptop
          floor of 5
```

The weaker number counts for more because **the bottleneck is what you feel**.
A very fast chip with 8 GB of memory cannot run a large model at all, and 128 GB
of memory does not speed up a slow chip.

The laptop penalty applies only to Max and Ultra chips. Those are the only ones
fast enough for heat to become the limit: a desktop holds its top speed
indefinitely, while a laptop of the same chip slows down once it warms up.
Slower chips never reach that ceiling, so they are not penalised.

No Mac scores below 5. Every Mac can run something.

## The five model sizes

A model's *size* is roughly how much it knows. Bigger models give better answers
and run more slowly. `Q4` in the labels means the model has been **compressed to
roughly a quarter of its original size** so that it fits in memory — this is the
normal way to run models on a Mac, and it costs a little accuracy.

Windrose assumes **65% of your total memory** is available to a model. The rest
goes to macOS and everything else you have open. A Mac that starts swapping to
disk becomes unusable long before it technically runs out of memory.

| Size class | Working-set memory |
|------------|-------------------:|
| 3B  | 2.5 GB |
| 7B  | 6 GB |
| 13B | 10 GB |
| 30B | 22 GB |
| 70B | 42 GB |

> **Working set, not download size.** These figures are larger than the file you
> download, because a running model also needs room for the conversation so far
> and for the program running it. A model whose *file* just fits in memory will
> not actually run. Sizing against download size is the most common way to get
> this wrong, and it is the difference between "it fits" and "it fits and works".

### Verdicts

Headroom is available memory ÷ working-set size:

| Headroom | Verdict | What it means |
|---------:|---------|---------------|
| ×1.5 or more | **Great** | Runs comfortably — a good everyday choice |
| ×1.2–1.5 | **OK** | Runs well, with room left for your other apps |
| ×1.0–1.2 | **Tight** | Only just fits — close other apps first, and expect it to slow down |
| under ×1.0 | **No** | Won't fit in memory — use a cloud provider for this size |

### Speed estimates

Where a model fits, Windrose shows a rough range in **tokens per second**. A
token is a little less than a word, so 30 tokens per second is roughly reading
speed for most people.

| | 3B | 7B | 13B | 30B | 70B |
|---|---|---|---|---|---|
| **Intel** | 3–6 | 2–4 | 1–2 | — | — |
| **Base** | 40–70 | 15–30 | 9–18 | 4–9 | 2–4 |
| **Pro** | 60–100 | 30–55 | 18–32 | 9–16 | 4–8 |
| **Max** | 80–130 | 45–75 | 28–45 | 14–24 | 7–12 |
| **Ultra** | 95–150 | 55–90 | 35–55 | 18–30 | 9–16 |

Sources: community llama.cpp and MLX benchmarks — **indicative only**. Real
speeds vary with the model, the settings, how long the conversation has run, and
how warm the machine is. A model that does not fit gets no speed estimate,
because a number for something that cannot load would be misleading.

## Where these numbers come from

The formula and both tables live in `src/scoring.rs` and are covered by tests
that pin the anchors, the penalty, and the requirement that more memory never
makes a model fit *worse*. If you change one, change this page to match.
