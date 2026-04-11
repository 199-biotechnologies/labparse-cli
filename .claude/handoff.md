# labparse Reliability Hardening — Session Handoff

**Date:** 2026-04-11
**Branch:** `worktree-labparse-p0-reliability` (worktree)
**Working dir:** `/Users/biobook/Code/longevity/labparse/.claude/worktrees/labparse-p0-reliability`
**Current state:** Sprints A, B, C complete. Clippy clean. 16/16 tests passing. All commits pushed to `main`.

## What this branch is for

Implementing GPT Pro's 12-step roadmap to make labparse safe for unattended clinical ingestion. The pipeline previously had silent corruption, fail-open status, and no validation. After A/B/C, the pipeline is materially safer but still missing multi-model intelligence (Sprint D).

## What's been done (in order)

### Pre-existing P0 fixes (already on main before this session)
- Comparators preserved (<, >, <=, >=)
- Locale-aware number parser exists (`parse_number`)
- Scale-changing unit mappings removed
- International qualifier injection removed (no more `glucosio` → `fasting glucose`)
- Unit provenance tracking via `UnitStatus` enum
- pdftotext + LLM (gpt-5.4-mini) pipeline added

### Sprint A — `d0e28e4` — Codex remaining importants
| ID | What | Where |
|----|------|-------|
| A1 | Thousands separator regex `\d{1,3}(?:[,.]\d{3})*(?:[,.]\d+)?\|...` | text_parser.rs:29,56,71 |
| A2 | Shared `detect_conflicts()` for text/csv (no more silent dropping) | parsers/mod.rs:124 |
| A3 | Lexical rejection → NeedsReview status | main.rs:149 |
| A4 | Row-proximity check (200 chars between value and marker name) | pdf_parser.rs:296 |
| A5 | Allow blank unit when catalog explicitly lists `""` | text/csv/pdf parsers |

### Sprint B — `2426667` — Catalog model + provenance
| ID | What | Where |
|----|------|-------|
| B1 | CBC differential split: `_pct` and `_abs` for neutrophils/lymph/mono/eos/baso (10 markers total) | data/biomarkers.toml:525-655 |
| B2 | Added `urine_creatinine`, HOMA-IR/ApoB-A1 aliases | data/biomarkers.toml |
| B3 | Spaced unit normalization (`x10^9 /L` → `x10^9/L`) | normalize.rs:391 |
| B4 | Added `page`, `raw_value_text`, `raw_unit`, `source_text` to `ParsedBiomarker` | normalize.rs:357 |
| B5 | Serialize new provenance fields in JSON output | output/json.rs |

**Result:** FBC report 10→20 markers (all 5 percentage CBC differentials now resolve)

### Sprint C — `115b3b2` — Validation + safety
| ID | What | Where |
|----|------|-------|
| C1 | Page-level LLM chunking on form feeds | pdf_parser.rs:`split_into_pages`, `llm_structure_text_paged` |
| C2 | Validator stack v1 (NEW FILE: `src/validate.rs`) | validate.rs |
| C3 | PHI sanitization before remote API calls | pdf_parser.rs:`sanitize_for_remote` |
| C4 | Multi-patient detection (refuses to merge mixed patient docs) | pdf_parser.rs:`extract_patient_id`, `verify_single_patient` |

**Validators in `src/validate.rs`:**
- Plausibility bands for 25+ biomarkers (HbA1c 99.9% → IMPOSSIBLE → partial_failure)
- CBC differential sum check (~100% ± 5)
- Friedewald equation (TC ≈ LDL + HDL + TG/5)
- TC/HDL ratio cross-check
- Reference range consistency (value outside range but not flagged → flag + warn)

### Cleanup — `8ff190b` — Clippy clean
- 0 clippy warnings
- All 16 tests passing
- Replaced manual `strip_prefix` with `.strip_prefix()`
- Simplified status logic
- Added `#[allow(dead_code)]` for public API kept for future use

## Current behavior on Patient B.K. test files

| Report | Markers | Parser | Status |
|--------|---------|--------|--------|
| Lipid Panel (3p) | 5/5 | pdf-llm | needs_review (3 markers outside ref range, LLM didn't flag — V5 catches) |
| Full Blood Count (4p) | 20/20 | pdf-text | success (10 abs + 10 pct CBC differentials) |
| Liver Panel (2p) | 5/5 | pdf-text | needs_review (regex noise unresolved) |
| Comprehensive (10p) | 47 resolved + 4 unresolved | pdf-llm | needs_review |

## Next steps — Sprint D + E

### Sprint D — Multi-model intelligence (architectural)

**D1. Progressive escalation router** (`src/main.rs`, `src/parsers/pdf_parser.rs`)
- Per-page model escalation: regex → `gpt-5-nano` ($0.05/M) → `gpt-5.4-mini` ($0.75/M) → `gpt-5.4` ($2.50/M)
- Only escalate pages where validation failed
- Add `escalation_history` to PageStatus
- Models available via OpenRouter (see `~/.claude/projects/-Users-biobook-Code-longevity/memory/reference_openrouter_models.md`)

**D2. OpenRouter vision path for scanned PDFs** (`src/parsers/pdf_parser.rs`)
- Currently broken: local Qwen3.5-9B VLM outputs reasoning text instead of JSON
- Add `call_openrouter_vision()` function
- Models: `z-ai/glm-5.1` ($1.26/M) or `openai/gpt-5-nano` ($0.05/M, has vision)
- Same schema, same validators as text path
- Convert PDF page to PNG via existing `pdf_to_images()`, base64 encode, send

**D3. Selective second-model verification** (`src/validate.rs`)
- Trigger on review predicates: comparator-bearing, missing/inferred unit, low-confidence resolution, validator failure, abnormal flag
- Run different provider (e.g. if OpenAI was primary, verify with `google/gemini-3.1-flash-lite-preview` $0.25/M)
- Compare results, flag disagreements as conflicts

**D4. Apple Vision OCR baseline** (was P0 in GPT Pro round 1, never added)
- Add as `pdftotext` alternative for image-only PDFs
- Use `swift` or `osascript` to call `Vision.framework` `RecognizeDocumentsRequest`
- Free, no API call, on-device

### Sprint E — Polish + completeness

**E1. Real `--verify` flag** (cli.rs, main.rs)
- Was removed for being fake, never re-added
- Should run dual extraction and require agreement

**E2. JSON v3 schema with audit_id** (output/json.rs, errors.rs)
- Add `audit_id: "sha256:..."` (hash of source document)
- Per GPT Pro round 1 schema design

**E3. Vendor fingerprinting** (catalog.rs or new vendor.rs)
- Detect known vendor formats (Singapore HealthHub, NUH, InnoQuest, Novi Health) by fingerprint
- Route deterministically to template parser instead of LLM

**E4. Unresolved feedback loop** (new file)
- Cluster unresolved markers by raw_name + unit pattern
- Promote stable patterns into catalog aliases
- Manual review queue

## Critical context

### Models (April 2026)
- **Banned per CLAUDE.md:** `o4-mini`, `o3`, `o4`, `gemini-2.5-pro`, `gemini-2.0-flash`, `gemini-1.5-pro`
- **Outdated, do NOT use:** `gpt-4.1-mini`, `gpt-4.1`, `gpt-4o` (previous gen)
- **Current:** `openai/gpt-5.4-mini` (primary text), `z-ai/glm-5.1` (vision), `google/gemini-3.1-flash-lite-preview` (verification), `openai/gpt-5.4` (escalation)
- See `~/.claude/projects/-Users-biobook-Code-longevity/memory/reference_openrouter_models.md`
- See `~/.claude/projects/-Users-biobook-Code-longevity/memory/feedback_model_selection.md`

### Codex review (gpt-5.4 xhigh)
The user wants Codex to review each sprint. Earlier review for Sprint A got stuck on stdin. To re-run:
```bash
codex exec -m gpt-5.4 --skip-git-repo-check --full-auto -c model_reasoning_effort="xhigh" "<prompt>" < /dev/null
```
Always use `< /dev/null` to prevent stdin hang.

Codex review prompts should be concise and specific. Don't ask Codex to "do" things — only review and report.

### GPT Pro reviews (saved in memory)
- Round 1: `~/.claude/projects/-Users-biobook-Code-longevity/memory/project_labparse_reliability_review.md`
- Round 2 (12-step roadmap): `~/.claude/projects/-Users-biobook-Code-longevity/memory/project_labparse_pipeline_review_2.md`
- Architecture review: `~/.claude/projects/-Users-biobook-Code-longevity/memory/project_labparse_extraction_architecture.md`

### Test data
- Real patient PDFs at `/Users/biobook/Health/Patient B.K./` (sanitized in code as `<PATIENT>` / `<ID>`)
- Best test files:
  - `2024-Apr-12 Lipid Panel.pdf` — 3 pages, HealthHub format
  - `2024-Mar-18 Full Blood Count.pdf` — 4 pages, full CBC with differentials
  - `2024-Mar-18 Liver Panel - Alb, ALT, ALP, AST, TBil.pdf` — 2 pages
  - `2024-Dec-02 HOMA-IR, Lipid, Liver, kidney, etc..pdf` — 10 pages, comprehensive panel

### Build / test commands
```bash
cd /Users/biobook/Code/longevity/labparse/.claude/worktrees/labparse-p0-reliability
cargo build --release 2>&1 | tail -3
cargo test 2>&1 | tail -5
cargo clippy --release 2>&1 | tail -3   # should show 0 warnings

# Test on real PDFs
./target/release/labparse "/Users/biobook/Health/Patient B.K./2024-Mar-18 Full Blood Count.pdf" --json 2>/dev/null | jq '{status, m: .data.biomarkers | length, doc_status: .metadata.document_status}'

# Test validators
./target/release/labparse --text "HbA1c 99.9%" --json   # should be partial_failure
./target/release/labparse --text "Total Cholesterol 200 mg/dL, LDL 50 mg/dL, HDL 30 mg/dL, Triglycerides 100 mg/dL" --json   # Friedewald failure
```

## Known issues / not yet fixed

### From Codex review (still open)
1. **Lexical verification proximity bypass** — A hallucinated marker can still pass if its name token also appears nearby. Mitigation idea: require exact substring match with the source line.
2. **Conflict equality ignores comparator** in PDF parser path (`pdf_parser.rs:485`) — `5.8` and `<5.8` should be a conflict. Text parser already handles this via `detect_conflicts`.
3. **Page accounting** — temp files keyed only by PID, no unique dir, no completeness check across page count.
4. **Prompt injection** — report text concatenated as instructions. PHI sanitization helps but not full mitigation.

### From GPT Pro round 2 (steps 9-12 not yet started)
- Step 9: Progressive escalation
- Step 10: Vision via OpenRouter
- Step 11: Selective verification
- Step 12: Vendor fingerprinting + unresolved feedback loop

## How to resume

1. Read `~/.claude/projects/-Users-biobook-Code-longevity/memory/MEMORY.md` for project context
2. Read GPT Pro round 2 review for full context: `memory/project_labparse_pipeline_review_2.md`
3. Read this handoff
4. Pick Sprint D first (D1 → D2 → D3 → D4) for the architectural completion
5. Then Sprint E for polish

User's directive from the previous session: **"do the sprint A and then the sprint B and then the sprint C and then a sprint D sequentially and autonomously, and at the end of each sprint get Codex to review it and to fix the things that were not done."**

So: continue with Sprint D autonomously. After each sub-step, run codex review with `< /dev/null` to avoid stdin hang. Fix critical findings before moving on.

## Files modified this session

| File | Changes |
|------|---------|
| `src/parsers/pdf_parser.rs` | pdftotext, LLM structuring, page chunking, verification, sanitization, multi-patient |
| `src/parsers/text_parser.rs` | Locale parser wired in, conflict-friendly extract, blank unit support |
| `src/parsers/csv_parser.rs` | Locale parser wired in, conflict detection, Unicode-safe |
| `src/parsers/mod.rs` | Shared `detect_conflicts()`, NeedsReview/Failed enum variants |
| `src/normalize.rs` | UnitStatus enum, ParsedBiomarker provenance fields, spaced unit handling |
| `src/output/json.rs` | DocumentStatus serialization, page_statuses, provenance fields |
| `src/main.rs` | Multi-stage routing, validate.rs integration, status propagation |
| `src/validate.rs` | NEW — validation stack v1 |
| `src/catalog.rs` | dead_code allows for future use |
| `data/biomarkers.toml` | CBC split, urine_creatinine, aliases |

## Final note

The user is frustrated by silent failures. They keep asking "are we missing anything from the previous review?" — always cross-check against the GPT Pro round 2 12-step roadmap before declaring done.

Don't add more models until validation can tell you when extraction is wrong. The validator stack (Sprint C) is now in place; Sprint D can safely add escalation because validation can drive it.
