# labparse

Lab result parser — PDF/CSV/text to structured biomarker JSON.

Part of the [Longevity CLI Suite](https://github.com/199-biotechnologies).

## Install

### Homebrew (macOS)

```bash
brew tap 199-biotechnologies/tap
brew install labparse
```

### Direct from GitHub (requires Rust + repo access)

```bash
# Install Rust if you don't have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install directly from GitHub (no clone needed)
cargo install --git https://github.com/199-biotechnologies/labparse.git

# Verify
labparse --version
```

### From source

```bash
git clone https://github.com/199-biotechnologies/labparse.git
cd labparse
cargo install --path .
```

### Vision Pipeline (PDF/image extraction)

The vision pipeline uses a local AI model to extract biomarkers from PDF images, photos, and screenshots.

```bash
# 1. Install the local vision model (6.6 GB, one-time download)
brew install ollama              # or: https://ollama.com/download
ollama pull qwen3.5:9b

# 2. (Optional) Install Rapid-MLX for faster inference (~30% faster)
curl -fsSL https://raw.githubusercontent.com/raullenchai/Rapid-MLX/main/install.sh | bash
~/.rapid-mlx/bin/pip install --no-user 'rapid-mlx[vision]'

# 3. Install Python dependencies for vision pipeline
pip install pymupdf sentence-transformers numpy pillow

# 4. Run on a lab PDF
python3 labparse_vision.py /path/to/bloodwork.pdf
```

### Fuzzy Biomarker Matching (embedding-based)

For matching messy OCR names to canonical biomarkers (e.g., "Glycosylated Hemoglobin A1C" → "hba1c"):

```bash
pip install sentence-transformers numpy
python3 fuzzy_match.py "Glycosylated Hemoglobin A1C"
# → hba1c (sim=0.841)
```

Pre-computed embeddings for 483 name variants are included in `data/embeddings/`.

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

Compose with [`biorange`](https://github.com/199-biotechnologies/biorange) for scoring:

```bash
labparse --text "HbA1c 5.8%, Fasting Glucose 95 mg/dL" --json | biorange --sex male --age 45 --json
```

Full vision pipeline:

```bash
# Extract from PDF image → score against longevity ranges
python3 labparse_vision.py bloodwork.pdf | biorange --sex male --age 45 --json
```

## Biomarker Coverage

148 biomarkers across 16 categories: metabolic, lipid, inflammation, kidney, liver, thyroid, hormones (male/female), blood count, iron, vitamins, cardiac, cancer markers, and more. All normalized from a standardized CSV embedded at compile time.

## Benchmarks

See [BENCHMARKS.md](BENCHMARKS.md) for performance data including:
- Cross-model validation (Qwen3.5-9B vs Gemini 3.1 Pro vs GPT-5.4)
- Optimization impact (3 min → 45s per page)
- ANE benchmarks on Apple Silicon

## License

Private — 199 Biotechnologies
