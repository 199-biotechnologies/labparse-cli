# labparse

Lab result parser — PDF/CSV/text to structured biomarker JSON.

Part of the [Longevity CLI Suite](https://github.com/199-biotechnologies).

## Usage

```bash
# Parse free-form text
labparse --text "HbA1c 5.8%, ApoB 95 mg/dL, LDL 130 mg/dL"

# Parse CSV
labparse bloodwork.csv

# Pipe from stdin
echo "Fasting Glucose 92 mg/dL, Triglycerides 150 mg/dL" | labparse --stdin

# JSON output (auto when piped, or force with --json)
labparse --text "HbA1c 5.8%" --json

# List known biomarkers
labparse biomarkers

# Agent discovery
labparse agent-info
```

## Output

```json
{
  "version": "1",
  "status": "success",
  "data": {
    "source": "text-input",
    "biomarkers": [
      {
        "name": "HbA1c",
        "standardized_name": "hba1c",
        "display_name": "Hemoglobin A1c",
        "value": 5.8,
        "unit": "%",
        "category": "metabolic"
      }
    ],
    "parse_warnings": []
  },
  "metadata": { "elapsed_ms": 1, "markers_found": 1, "parser": "text" }
}
```

## Pipeline

Compose with `biorange` for scoring:

```bash
labparse --text "HbA1c 5.8%, Fasting Glucose 95 mg/dL" --json | biorange --sex male --age 45 --json
```

## Install

```bash
cargo install --path .
```

## Biomarker Coverage

148 biomarkers across 16 categories, normalized from a standardized CSV embedded at compile time. Common aliases mapped to canonical names (e.g., "Hemoglobin A1c" → "hba1c").

## Vision Pipeline (WIP)

For PDF/image extraction using local vision models (Qwen3.5-9B), see `labparse_vision.py` in the [longevity](https://github.com/199-biotechnologies/longevity) repo. Benchmarks: [BENCHMARKS.md](../BENCHMARKS.md).
