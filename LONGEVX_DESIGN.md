# longevx — Patient Biomarker Database CLI

> Design notes for next session. Rust CLI, SQLite storage.

## Purpose

Store biomarker history per patient. Enable longitudinal tracking, trend analysis, and data export for scoring (biorange) and protocol generation (protogen, future).

## Commands

```bash
# Patient management
longevx init "Jensen Fernandez" --sex male --dob 2006-08-02
longevx list [--json]
longevx show jensen-fernandez [--json]
longevx archive jensen-fernandez

# Data ingestion (accepts labparse JSON output)
longevx add jensen-fernandez bloodwork.json --date 2026-02-11
labparse --text "HbA1c 5.8%" --json | longevx add jensen-fernandez
labparse_vision.py labs.pdf | longevx add jensen-fernandez

# Querying
longevx history jensen-fernandez --biomarker creatinine,hba1c,alt
longevx trend jensen-fernandez [--biomarker hba1c]        # rate of change
longevx latest jensen-fernandez [--json]                  # most recent values
longevx compare jensen-fernandez --from 2025-06 --to 2026-03

# Export (for downstream tools)
longevx export jensen-fernandez --format json              # all timepoints
longevx export jensen-fernandez --latest --format json | biorange --sex male --age 19
longevx export jensen-fernandez --summary                  # for LLM clinical reasoning

# Database
longevx stats                                              # patient count, total records
longevx backup [--output backup.db]
```

## Storage

**SQLite** at `~/.longevx/longevx.db` (configurable via `LONGEVX_DB` env var or `--db` flag).

### Schema

```sql
CREATE TABLE patients (
    id INTEGER PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,          -- "jensen-fernandez"
    name TEXT NOT NULL,                 -- "Jensen Fernandez"
    sex TEXT,                           -- "male" | "female"
    dob TEXT,                           -- "2006-08-02" ISO date
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    archived INTEGER DEFAULT 0
);

CREATE TABLE lab_sessions (
    id INTEGER PRIMARY KEY,
    patient_id INTEGER NOT NULL REFERENCES patients(id),
    date TEXT NOT NULL,                 -- "2026-02-11" collection date
    source TEXT,                        -- "bloodwork.pdf" or "manual"
    provider TEXT,                      -- "London Blood Tests UK"
    ingested_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE biomarkers (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES lab_sessions(id),
    name TEXT NOT NULL,                 -- "Haemoglobin (Hb)"
    standardized_name TEXT NOT NULL,    -- "hemoglobin"
    value REAL NOT NULL,                -- 150.0
    unit TEXT NOT NULL,                 -- "g/L"
    reference_range TEXT,               -- "134 - 166"
    flagged INTEGER DEFAULT 0,          -- 1 if outside reference range
    confidence REAL DEFAULT 1.0         -- from vision extraction
);

CREATE INDEX idx_biomarkers_patient ON biomarkers(session_id);
CREATE INDEX idx_biomarkers_name ON biomarkers(standardized_name);
CREATE INDEX idx_sessions_patient ON lab_sessions(patient_id);
CREATE INDEX idx_sessions_date ON lab_sessions(date);
```

## Rust Crates

- `rusqlite` — SQLite bindings (with `bundled` feature = zero system deps)
- `clap` 4.5+ — CLI (same pattern as labparse/biorange)
- `serde` + `serde_json` — JSON I/O
- `comfy-table` + `owo-colors` — human output
- `chrono` — date handling
- `slug` — name-to-slug conversion ("Jensen Fernandez" → "jensen-fernandez")

## Key Design Decisions

1. **SQLite, not files.** Thousands of patients × hundreds of biomarkers = millions of rows. SQLite handles this instantly. Single file, zero config, portable.

2. **Accepts labparse output directly.** `labparse --json | longevx add patient` — the JSON format is the contract between the two tools.

3. **Standardized names are mandatory.** Every biomarker gets a `standardized_name` from labparse's normalization + fuzzy matching. This is what makes cross-lab, cross-time queries work.

4. **Confidence scores.** Vision-extracted biomarkers carry a confidence score. Dual-verified (OCR + vision agree) = 0.95+. Single source = 0.7. This propagates to downstream tools.

5. **No ML in longevx.** Pure database operations. All intelligence lives in labparse (extraction), biorange (scoring), and future protogen (protocols).

6. **Trend analysis is math, not AI.** Rate of change = (value_now - value_then) / days_between. Acceleration = second derivative. No LLM needed.

## Example Output

```
$ longevx history jensen-fernandez --biomarker creatinine

  Jensen Fernandez — Creatinine (umol/L)
  ────────────────────────────────────────
  2025-06-15    98   ▐██████████████░░░░░░  (ref: 59-104)
  2025-09-20   101   ▐███████████████░░░░░
  2026-01-10   104   ▐████████████████░░░░
  2026-02-11   106 ⚠ ▐█████████████████░░░  ABOVE REF

  Trend: +2.7 umol/L per quarter (rising)
  Velocity: accelerating (+0.3/quarter²)
```

## Pipeline Integration

```
Photo/PDF → labparse_vision.py → labparse JSON
                                      ↓
                              longevx add patient
                                      ↓
                              longevx export --latest --json
                                      ↓
                              biorange --sex male --age 19
                                      ↓
                              scored assessment JSON
                                      ↓
                              (future) protogen → protocol docs
```
