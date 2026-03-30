<div align="center">

# labparse

**Parse lab results from PDF, CSV, and text into structured biomarker JSON**

<br />

[![Star this repo](https://img.shields.io/github/stars/199-biotechnologies/labparse-cli?style=for-the-badge&logo=github&label=%E2%AD%90%20Star%20this%20repo&color=yellow)](https://github.com/199-biotechnologies/labparse-cli/stargazers)
&nbsp;&nbsp;
[![Follow @longevityboris](https://img.shields.io/badge/Follow_%40longevityboris-000000?style=for-the-badge&logo=x&logoColor=white)](https://x.com/longevityboris)

<br />

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Homebrew](https://img.shields.io/badge/Homebrew-FBB040?style=for-the-badge&logo=homebrew&logoColor=black)](https://brew.sh/)
[![Apple Silicon](https://img.shields.io/badge/Apple_Silicon-native-000000?style=for-the-badge&logo=apple&logoColor=white)](https://support.apple.com/apple-silicon)
[![License](https://img.shields.io/badge/License-Proprietary-blue?style=for-the-badge)](LICENSE)
[![JSON Output](https://img.shields.io/badge/Output-JSON_v2-orange?style=for-the-badge&logo=json)](https://www.json.org/)

---

Every lab report uses a different format. Different names for the same marker. Different units. Different layouts entirely. You end up writing one-off scripts or copying values into a spreadsheet by hand.

labparse reads any of them and gives you clean JSON -- standardized names, units, confidence scores. 203 markers across 23 categories, matched through a 7-step fuzzy normalization pipeline. PDF extraction runs a local vision model on your machine. Your medical data never leaves your device.

[Install](#install) | [How It Works](#how-it-works) | [Quick Start](#quick-start) | [Features](#features) | [Contributing](#contributing)

</div>

## Why This Exists

Lab reports come in dozens of formats. Quest prints things one way, LabCorp another, and international labs do whatever they want. The marker names are all over the place too -- "Hemoglobin A1c" vs "HbA1c" vs "Glycated Hemoglobin" all mean the same thing. Getting structured data out of any of these means tedious manual work that nobody wants to do twice.

labparse handles it. Point it at a PDF or CSV, or paste the text straight in. You get back standardized JSON with every marker resolved to a canonical name -- under 2 milliseconds for text input.

## Install

### Homebrew (macOS)

```bash
brew tap 199-biotechnologies/tap
brew install labparse
```

### Cargo

```bash
cargo install --git https://github.com/199-biotechnologies/labparse-cli.git
```

### From source

```bash
git clone https://github.com/199-biotechnologies/labparse-cli.git
cd labparse-cli
cargo install --path .
```

### PDF vision setup (optional)

PDF extraction uses a local Qwen3.5-9B vision model. One-time setup:

```bash
brew install ollama
ollama pull qwen3.5:9b
```

## How It Works

```
                          ┌──────────────────────────┐
   Lab PDF  ──────┐      │        labparse           │
                  │      │                            │
   CSV file ──────┼─────▶│  7-step normalization      │──────▶  Biomarker JSON v2
                  │      │  203 markers, 1160 aliases │        (standardized names,
   Raw text ──────┘      │  23 categories             │         units, confidence)
                          └──────────────────────────┘
                                     │
                                     ▼
                           Local vision model
                          (PDFs only, on-device)
```

Text and CSV go through a regex-based parser. PDFs get rendered to images and read by a local vision model (Qwen3.5-9B via Ollama). Every extracted marker name runs through a 7-step normalization pipeline: lowercase, strip specimen prefix, strip method suffix, remove parentheticals, British-to-American spelling, CamelCase split, noise removal, then exact catalog lookup.

The output is JSON v2 with `resolved`, `confidence`, and `resolution_method` fields on each marker. Unmatched markers land in a separate `unresolved[]` array -- nothing gets silently dropped.

## Quick Start

**Parse text directly:**

```bash
labparse --text "HbA1c 5.8%, ApoB 95 mg/dL, LDL-C 118 mg/dL"
```

**Parse a lab PDF:**

```bash
labparse bloodwork.pdf
```

**Pipe from stdin:**

```bash
echo "Fasting Glucose 92 mg/dL, Triglycerides 68 mg/dL" | labparse --stdin
```

**Get JSON output and pipe to another tool:**

```bash
labparse --text "HbA1c 5.8%" --json | labassess --sex male --age 45
```

Output auto-switches to JSON when piped. Human-readable tables show up on the terminal.

**List all known biomarkers:**

```bash
labparse biomarkers
labparse biomarkers --category lipid
```

## Features

| Feature | Detail |
|---|---|
| **203 biomarkers** | 1160 aliases across 23 clinical categories |
| **PDF extraction** | Local Qwen3.5-9B vision model, no cloud API calls |
| **7-step fuzzy matching** | Handles OCR errors, alternate spellings, international naming |
| **JSON v2 output** | Confidence scores, resolution method, unresolved marker array |
| **Dual output mode** | Human-readable tables on TTY, JSON when piped |
| **Cross-verification** | Optional `--verify` flag to validate extraction against a second model |
| **Fast** | ~2ms text parsing, ~5MB memory footprint |
| **Agent-friendly** | `agent-info` subcommand for AI tool discovery |
| **Composable** | Unix-style piping to labassess, labstore, and other tools |

**Supported input formats:** PDF, CSV, TSV, and free-form text (pasted lab results, OCR output, clinical notes).

**Supported categories:** metabolic, lipid, inflammation, hematology, iron, kidney, liver, electrolytes, thyroid, hormone, nutritional, cardiac, cancer markers, immunology, cardiovascular, neurological, coagulation, urinalysis, body composition, functional, sleep, cardiovascular imaging, pulmonary.

## Part of the Longevity CLI Suite

labparse is one tool in a set of composable Rust CLIs for clinical biomarker analysis:

```
Lab PDF/CSV/text → labparse → Biomarker JSON
                                ├→ labstore  → SQLite patient database
                                └→ labassess → Longevity-scored assessment
```

Each CLI does one job and pipes JSON to the next. Built by [199 Biotechnologies](https://github.com/199-biotechnologies).

## Contributing

Pull requests are welcome, especially for the biomarker catalog in `data/biomarkers.toml` -- adding new markers or aliases is the fastest way to contribute. For anything bigger, open an issue first so we can talk it through.

## License

Proprietary -- Copyright (c) 2026 Boris Djordjevic, 199 Biotechnologies & Paperfoot AI

---

<div align="center">

Built by [Boris Djordjevic](https://github.com/longevityboris) at [199 Biotechnologies](https://github.com/199-biotechnologies) | [Paperfoot AI](https://paperfoot.ai)

<br />

**If this is useful to you:**

[![Star this repo](https://img.shields.io/github/stars/199-biotechnologies/labparse-cli?style=for-the-badge&logo=github&label=%E2%AD%90%20Star%20this%20repo&color=yellow)](https://github.com/199-biotechnologies/labparse-cli/stargazers)
&nbsp;&nbsp;
[![Follow @longevityboris](https://img.shields.io/badge/Follow_%40longevityboris-000000?style=for-the-badge&logo=x&logoColor=white)](https://x.com/longevityboris)

</div>
