# Longevity CLI Suite — Benchmarks & Optimization Notes

> Tested on M4 Max 64GB, macOS 26.2, March 2026

## Models Tested

| Model | Backend | Size | Vision | Installed |
|-------|---------|------|--------|-----------|
| Qwen3.5-9B Q4_K_M | Ollama (GGUF) | 6.6 GB | Yes | `ollama run qwen3.5:9b` |
| Qwen3.5-9B 4-bit | MLX | 5.6 GB | Yes | `mlx-community/Qwen3.5-9B-4bit` |
| Qwen3.5-9B 6-bit | MLX | 7.7 GB | Yes | `mlx-community/Qwen3.5-9B-6bit` |
| Qwen3.5-9B 8-bit | MLX | 9.7 GB | Yes | `mlx-community/Qwen3.5-9B-8bit` |

## Inference Speed — Qwen3.5-9B on M4 Max 64GB

### Generation Speed (text-only)

| Engine | tok/s | Notes |
|--------|-------|-------|
| **Rapid-MLX** (4-bit) | **~29 tok/s** | +32% vs stock MLX; prefix caching; OpenAI-compatible API |
| Stock MLX (6-bit, mlx_vlm) | ~22 tok/s | Baseline; tested with vision |
| Ollama (Q4_K_M) | ~24-30 tok/s (est) | Could not benchmark cleanly due to resource contention |

### Vision Inference (lab report page extraction)

Test: 4-page PDF, Jensen Fernandez blood test, page 1 = haematology + biochemistry

| Engine | Image DPI | Vision Tokens | Prefill Time | Generation | Total |
|--------|-----------|---------------|-------------|------------|-------|
| Stock MLX 6-bit | 300 DPI (2481x3508) | 8,613 | 2m 33s @ 55.7 tok/s | 21s @ 22 tok/s | **3m 7s** |
| Stock MLX 6-bit | **150 DPI (1241x1754)** | ~2,000 (est) | ~36s (est) | ~21s | **~57s** |
| Rapid-MLX 4-bit (text) | N/A | N/A | N/A | 29 tok/s | N/A |

### Rapid-MLX Benchmark (M3 Ultra 256GB, published numbers)

| Model | tok/s | TTFT (cached) |
|-------|-------|---------------|
| Qwen3.5-9B | 108 | — |
| Qwen3.5-35B-A3B | 83 | — |
| Qwen3.5-122B-A10B | 57 | — |
| Qwen3-Coder-Next 80B | 74 | 0.10s |
| Qwen3.5-397B-A17B | 4.36 (flash-moe) | — |

## Optimizations Applied (Zero Quality Loss)

### 1. Image Downscaling (4x fewer vision tokens)

Lab PDFs rendered at 300 DPI produce 2481x3508 images = 8,613 vision tokens (2.5 min prefill).
At 150 DPI: 1241x1754 = ~2,000 tokens (~36s prefill). Lab text is 10-12pt, perfectly readable at 150 DPI.

**Impact:** ~4x faster prefill per page.

### 2. Rapid-MLX with Prefix Caching

Rapid-MLX (`rapid-mlx serve qwen3.5-9b --mllm`) caches the KV state for repeated prompts.
When processing multiple pages of the same lab report, the extraction prompt is identical —
only image tokens change. Pages 2-7 skip prompt prefill entirely.

**Impact:** ~30% faster generation + near-zero TTFT on subsequent pages.

### 3. No-Think Mode

Qwen3.5 defaults to thinking mode (`<think>...</think>` before responding).
For structured extraction tasks (JSON output), thinking is unnecessary overhead.
Disabled via `/no_think` prefix in prompt.

**Impact:** ~2x fewer tokens generated (no reasoning chain).

### Combined Effect

| Scenario | Before Optimization | After Optimization | Speedup |
|----------|--------------------|--------------------|---------|
| Single page (300 DPI, thinking) | ~3 min | ~45s | **4x** |
| 7-page report (300 DPI, no cache) | ~21 min | ~4 min | **5x** |
| 7-page report (cached, 150 DPI) | ~21 min | ~2.5 min | **8x** |

## Cross-Model Validation

All models tested on Jensen Fernandez page 1 (25 biomarkers):

| Model | Biomarkers Found | Values Correct | Units Correct | Discrepancies |
|-------|-----------------|----------------|---------------|---------------|
| **Qwen3.5-9B** (Ollama, vision) | 25 | 25/25 | 25/25 | None |
| **Gemini 3.1 Pro** (API, text) | 25 | 25/25 | 25/25 | None |
| **GPT-5.4 / Codex** (API, text) | 25 | 25/25 | 25/25 | None |
| **pymupdf text extraction** | 25 | 22/25 | 22/25 | 3 rows misaligned (PLT/WBC/MPV) |

**Verdict:** All three LLMs produce identical results. Zero discrepancies on name, value, unit, or reference range. Text extraction is fastest but brittle on edge cases.

## Apple Neural Engine (ANE) Benchmarks

Direct ANE access via reverse-engineered `_ANEClient` private APIs ([maderix/ANE](https://github.com/maderix/ANE)).

### ANE In-Memory Benchmark (M4 Max)

| Config | Weight (MB) | ms/eval | TFLOPS |
|--------|-------------|---------|--------|
| 256ch x 64sp | 0.1 | 0.134 | 0.06 |
| 512ch x 64sp | 0.5 | 0.125 | 0.27 |
| 1024ch x 64sp | 2.0 | 0.144 | 0.93 |
| 2048ch x 64sp | 8.0 | 0.203 | **2.64** |
| 3072ch x 64sp | 18.0 | 0.343 | **3.53** |
| 4096ch x 64sp | 32.0 | 1.084 | 1.98 |

### ANE Peak Performance (M4 Max)

| Config | Weight (MB) | ms/eval | TFLOPS |
|--------|-------------|---------|--------|
| 128x conv 512ch sp64 | 64 | 0.446 | **9.62** |
| 96x conv 512ch sp64 | 48 | 0.371 | **8.68** |
| 64x conv 512ch sp64 | 32 | 0.295 | 7.28 |
| 128x conv 384ch sp64 | 36 | 0.316 | 7.65 |

**Peak: 9.62 TFLOPS** on M4 Max ANE. This is ~50% of the theoretical 19 TFLOPS, significantly above the published ~5-9% utilization in the original project.

### ANE Relevance for Lab Parsing

The ANE could theoretically handle the prefill phase (matrix multiplies for attention) while GPU handles token generation. The hybrid-ane-mlx-bench project tested this:

- ANE batch prefill: 268 tok/s on 0.8B model (11.3x speedup over sequential)
- GPU power drops from 62W to 0.22W during ANE prefill
- **Caveat:** On macOS 26.3, CoreML `compute_units=ALL` silently routes to GPU, not ANE
- **Caveat:** Private APIs crash after ~119 compilations (requires process restart)
- **Caveat:** ANE ISA changes between chip generations (M1/M2/M3/M4 all different)

**Status:** ANE is proven to work on M4 Max but not yet production-ready for our pipeline. Watching for WWDC 2026 "Core AI" framework which may expose stable ANE APIs.

## Infrastructure Stack

| Component | Tool | Version | Purpose |
|-----------|------|---------|---------|
| Lab PDF parsing (v1) | `labparse` | 0.1.0 | Regex text extraction (CSV/text/stdin) |
| Lab PDF parsing (v2) | `labparse_vision.py` | WIP | Vision model extraction with dual verification |
| Biomarker scoring | `biorange` | 0.1.0 | Range checking, derived values, patterns, red flags |
| Local vision model | Qwen3.5-9B | Feb 2026 | Native multimodal, Apache 2.0 |
| Inference engine | Rapid-MLX | 0.3.12 | Fastest Apple Silicon inference, prefix caching |
| Fallback inference | Ollama | 0.17.6 | Simpler setup, GGUF backend |
| API verification | Gemini 3.1 Pro | Mar 2026 | Cross-validation ($0.01/page) |
| API verification | GPT-5.4 (Codex) | Mar 2026 | Cross-validation ($0.04/page) |
| PDF rendering | pymupdf (fitz) | 1.27.2 | PDF to image + text extraction |
| ANE research | maderix/ANE | Latest | Neural Engine benchmarking |
| ANE Rust engine | ncdrone/rustane | Latest | Rust ANE training/inference (experimental) |
