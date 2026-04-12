# Session Handoff

**Date:** 2026-04-12
**Session:** labparse Sprint D reliability hardening — status honesty, dead VLM gate, pdftotext cleanup, catalog triage, 613-alias import from private dataset
**Context usage at handoff:** ~60%

---

## Active Plan

There is no formal plan file. The work is driven by a GPT Pro benchmark analysis (Q1-Q8) that produced an ordered 8-step Sprint D roadmap. The full analysis was provided by the user in conversation and is summarized below.

**Revised Sprint D roadmap (from GPT Pro benchmark analysis):**

1. ~~D1 — Shared `assess_document_status()` across all parsers~~ **DONE**
2. ~~D2 — Kill dead local VLM hang (`--experimental-vlm` gate)~~ **DONE**
3. ~~D3 — Deterministic pdftotext cleanup (`clean_pdftotext()`)~~ **DONE**
4. ~~D4a — Catalog triage (aliases, disambiguation, specimen suffix)~~ **DONE**
5. ~~D4a+ — Private dataset alias import (613 aliases, Codex-reviewed)~~ **DONE**
6. **D4b — Completeness accounting** (block splitting, truncation warnings, `PageExtractStatus::Partial`) — **NEXT**
7. **D5 — Row-grounded extraction contract** (LLM returns exact `value_text`/`unit_text`/`source_text`, parsed locally) — **NEXT**
8. D6 — Per-page text escalation (regex → mini → full, page-level)
9. D7 — OpenRouter vision path (Qwen primary, GLM fallback)
10. D8 — Selective second-model verification (Gemini Flash Lite for flagged rows)

**Plan status:** D1-D4a+ complete. D4b and D5 are next (independent, no external deps). D6-D8 require OpenRouter integration.

## What Was Accomplished This Session

### Sprint D1 — Status honesty (`be289c2`)
- Added `ExtractionAudit` struct and `assess_document_status()` to `src/validate.rs`
- Catches false-success: zero resolved, unresolved dominates, ambiguous, inferred units, truncated pages, lexical rejections, conflicts
- Runs as final gate in `validate()` after all sub-validators
- 6 new unit tests for status assessment

### Sprint D2 — Dead VLM gate (`be289c2`)
- Added `--experimental-vlm` CLI flag to `src/cli.rs`
- Scanned PDFs (no text layer) and direct image input fail fast with clear error message
- Previously hung 10-22s on broken local Qwen3.5-9B VLM

### Sprint D3 — Pdftotext cleanup (`be289c2`)
- Added `clean_pdftotext()` to `src/parsers/pdf_parser.rs`
- Strips: page counters, generated timestamps, "computer generated" footers, "Lab Test Result" headers (+ 2 lines: name + NRIC), facility lines, date lines, "Ref. Range:" labels, "Serum Indices", haemolysis/lipaemia lines, remarks sections
- Wired into main.rs before regex parsing
- Result: Liver 14→0 unresolved, FBC 54→0, Lipid now uses fast `pdf-text` path

### Sprint D4a — Catalog triage (`6f75695`)
- Added aliases: "zinc serum", "microalbumin random", "total white cell count"
- Added CBC differential disambiguation (bare "neutrophils" → unit-based _pct/_abs routing)
- Added spaceless specimen suffix stripping (" serum", " plasma", " whole blood")
- Result: Comprehensive panel 55→75 resolved markers

### D4a+ — Private dataset alias import (`0897030`, `fc60cd7`)
- Cross-referenced 1,370 lab variables from private clinical dataset against our 262-marker catalog
- Matched 144 markers by LOINC + name, imported 613 new synonym/alias mappings
- International names: German, Czech, vendor-specific formats
- Codex review caught and fixed:
  - P0: TAS alias collision (ASO vs Antioxidant Status)
  - P1: 3 wrong LOINC codes (ApoE→1886-1, TBG→3021-3, IgE→19113-0)
  - P1: 3 clinically wrong aliases (2hr glucose under fasting, cortisol PM under AM, urine hemoglobin under VEGF)
  - P1: Removed " random" from spaceless suffix stripping (Glucose Random → fasting_glucose)
  - P2: 5 unit-bearing aliases removed
- Final: 0 alias collisions, all tests pass

### Codex review fixes (`5004478`)
- Bounded `skip_header_lines` counter (was open-ended `skip_next_nric`)
- Wired `lexical_rejections` from main.rs into ParseResult for shared status gate
- Empty extraction (resolved=0, unresolved=0) → NeedsReview instead of Complete

## Key Decisions Made

1. **Safety before models.** The GPT Pro analysis reordered Sprint D: fix status honesty, kill dead paths, clean noise, then add models. This was the right call — D3 alone eliminated 54 unresolved on FBC without touching the LLM.

2. **`clean_pdftotext()` uses bounded header skipping.** After "Lab Test Result", skip exactly 2 lines (patient name + NRIC). The original open-ended `skip_next_nric` flag could eat real analyte rows (Codex caught this).

3. **CBC differential disambiguation uses unit, not context.** Bare "Neutrophils" with % → `_pct`, with `x10^9/L` → `_abs`. This is the safest approach since the resolver has no page/section context.

4. **Qualitative/OOS markers stay unresolved by design.** Bare urinalysis names ("pH", "Nitrite", "Blood"), hepatitis serology, bare "White Blood Cells"/"Red Blood Cells" are NOT added as aliases because they're ambiguous without specimen context.

5. **Private dataset alias import requires collision scanning.** The import script matched by LOINC code, but several LOINC codes in the labparse catalog were wrong, causing cross-contamination (ApoE got ApoB aliases, TBG got Transferrin aliases, IgE got IgA aliases). Always run the collision scanner after any bulk alias import.

6. **Removed " random" from spaceless suffix stripping.** "Random" is a specimen collection qualifier, not a specimen type. Stripping it caused "Glucose Random" to resolve to fasting_glucose.

## Current State

- **Branch:** `worktree-labparse-p0-reliability` (worktree) — fully merged to `main`
- **Last commit:** `fc60cd7` — fix: address Codex review — LOINC codes, wrong aliases, TAS collision
- **Uncommitted changes:** None
- **Tests passing:** Yes — 40 unit + 16 integration tests
- **Clippy:** 0 warnings
- **Build:** Clean release build

### Test results on Patient B.K. files:

| Report | Markers | Unresolved | Status | Parser |
|--------|---------|------------|--------|--------|
| Liver Panel (2p) | 5 | 0 | success | pdf-text |
| Full Blood Count (4p) | 20 | 0 | success | pdf-text |
| Lipid Panel (3p) | 5 | 0 | success | pdf-text |
| Comprehensive (10p) | 71 | 5 | needs_review | pdf-llm |

Remaining 5 unresolved on comprehensive are genuinely OOS: Hepatitis Bs Antigen/Antibody, Anti-HAV Total, pH, bare urinalysis WBC/RBC.

## What to Do Next

1. Read this handoff document
2. Read the GPT Pro benchmark analysis context from memory: `~/.claude/projects/-Users-biobook-Code-longevity/memory/project_labparse_pipeline_review_2.md`
3. Read model reference: `~/.claude/projects/-Users-biobook-Code-longevity/memory/reference_openrouter_models.md`

### D4b — Completeness accounting
4. Add `split_dense_page()` to `src/parsers/pdf_parser.rs` — split pages exceeding 30k chars on double newlines, section headers ("Remarks", "Interpretation"), and repeated "Results:" anchors
5. Replace silent 30k char truncation in `llm_structure_page()` with block-level extraction + `PageExtractStatus::Partial` warning
6. Wire `Partial` status into `assess_document_status()` (already handled — `truncated_pages > 0` → NeedsReview)

### D5 — Row-grounded extraction contract
7. Change LLM prompt in `pdf_parser.rs` to require exact `value_text`, `unit_text`, `source_text` fields (not parsed numeric `value`)
8. Add local parsing of `value_text` → numeric value after extraction (comparator detection, locale-aware number parsing)
9. Strengthen `verify_against_source()` to require row-level match (name + value + unit from same source line), not just 200-char proximity

### D6-D8 — OpenRouter integration (requires API key)
10. Add `call_openrouter()` HTTP client to `pdf_parser.rs` (uses `OPENROUTER_API_KEY` env var)
11. Implement per-page escalation policy (`PageSignal` → `EscalationAction`)
12. Add OpenRouter vision path for scanned PDFs (Qwen3-VL primary, GLM-5.1 fallback)
13. Add selective verification with Gemini Flash Lite for flagged rows

### Codex review after each step
14. After each D-step, run: `codex exec -m gpt-5.4 --skip-git-repo-check --full-auto -c model_reasoning_effort="xhigh" "<review prompt>" < /dev/null`
15. Fix all P0/P1 findings before proceeding

## Files to Review First

1. `src/validate.rs` — shared `ExtractionAudit`, `assess_document_status()`, all validators
2. `src/main.rs` — routing logic (regex → LLM → VLM), status propagation, `--experimental-vlm` gate
3. `src/parsers/pdf_parser.rs` — `clean_pdftotext()`, `llm_structure_text_paged()`, `verify_against_source()`, LLM prompts, 30k truncation
4. `src/catalog.rs` — `normalize_pipeline()`, specimen suffix stripping, disambiguation resolution
5. `data/biomarkers.toml` — 262 markers, 613 imported aliases, disambiguation table

## Gotchas & Warnings

- **Codex needs `< /dev/null`** — without it, codex exec hangs waiting on stdin
- **No CI/CD** — all tests are local-only. Run `cargo test` in the worktree before pushing.
- **The worktree is at** `/Users/biobook/Code/longevity/labparse/.claude/worktrees/labparse-p0-reliability` — NOT the main labparse dir
- **Main labparse dir** at `/Users/biobook/Code/longevity/labparse` has `main` checked out and is fully merged
- **`llm_structure_page()` still silently truncates at 30k chars** — this is D4b's target
- **LLM prompt asks for numeric `value` not `value_text`** — this is D5's target, the root cause of weak hallucination grounding
- **`verify_against_source()` only checks 200-char proximity** — too weak, accepts cross-row borrowing on dense pages
- **Models (April 2026):** `openai/gpt-5.4-mini` (primary text), `z-ai/glm-5.1` (vision), `google/gemini-3.1-flash-lite-preview` (verification), `openai/gpt-5.4` (escalation). Banned: o4-mini, o3, o4, gemini-2.5-pro, gemini-2.0-flash, gemini-1.5-pro
- **Private dataset alias import can introduce LOINC cross-contamination** — always run collision scanner after bulk imports
- **"Random" is NOT a specimen type** — don't strip it from marker names (Glucose Random ≠ Fasting Glucose)
- **Test PDFs at** `/Users/biobook/Health/Patient B.K./` — these are real patient data, always sanitize before sending to remote APIs
- **The user does NOT want OpenCures mentioned** in commits or public-facing text. Use "private dataset" or "private lab variable dataset".
