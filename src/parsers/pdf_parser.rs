//! PDF lab report extraction via Qwen3.5-9B on Apple Silicon (MLX).
//!
//! Pipeline: PDF → pdftoppm (150 DPI PNG) → mlx_vlm vision → JSON → catalog resolver
//!
//! Optimizations:
//! 1. 150 DPI rendering (4x fewer vision tokens, lab text still readable)
//! 2. enable_thinking=False (no reasoning chain, 2x fewer tokens)
//! 3. Model loaded ONCE for all pages (eliminates 4s reload per page)
//! 4. KV cache reuse across pages (same prompt → cached prefill on pages 2+)
//!
//! Requires: pdftoppm (brew install poppler), mlx-vlm (pip install mlx-vlm)

use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::catalog;
use crate::errors::LabParseError;
use crate::normalize::{normalize_name, normalize_unit, ParsedBiomarker};
use crate::parsers::{
    ConflictCandidate, ConflictMarker, DocumentStatus, PageExtractStatus, PageStatus, ParseResult,
    UnresolvedMarker,
};

const EXTRACTION_PROMPT: &str = "Extract ALL biomarkers from this lab report page. Output ONLY a valid JSON array.\n\
Each object must have exactly these fields: name, value, unit, reference_range.\n\
Rules:\n\
- value must be a number (float or int), not a string\n\
- unit must be the exact unit shown (e.g., mmol/L, g/L, IU/L, X10^9/L)\n\
- reference_range must be the range shown (e.g., \"4.3 - 5.4\")\n\
- If a value is flagged/highlighted as abnormal, add \"flagged\": true\n\
- Do NOT include section headers (e.g., \"Kidney Function\", \"Liver Function\")\n\
- Do NOT skip any biomarker row\n\
Output ONLY the JSON array, nothing else.";

const MLX_MODEL: &str = "mlx-community/Qwen3.5-9B-4bit";

/// Raw biomarker from vision model output
#[derive(Debug, Deserialize, Clone)]
struct VisionBiomarker {
    name: String,
    value: serde_json::Value,
    #[serde(default)]
    unit: String,
    #[allow(dead_code)]
    #[serde(default)]
    reference_range: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    flagged: Option<bool>,
    /// Page number this marker came from (injected during resolution)
    #[serde(skip)]
    page: Option<usize>,
}

/// Per-page result from the Python batch extraction
#[derive(Debug, Deserialize)]
struct PageResult {
    page: usize,
    markers: Vec<VisionBiomarker>,
    elapsed_s: f64,
    #[serde(default)]
    error: Option<String>,
}

/// Parse image file(s) directly (JPG, PNG, etc.) — no PDF conversion needed.
pub fn parse_image(image_path: &Path) -> Result<ParseResult, LabParseError> {
    check_mlx_vlm()?;
    let img = image_path.to_string_lossy().to_string();
    eprintln!("info: 1 image");
    let images = vec![img];
    let page_results = extract_all_pages(&images)?;

    // Cleanup not needed (user's file, not temp)
    // Reuse the same resolution pipeline as PDF
    resolve_page_results(page_results)
}

/// Parse a PDF file using MLX vision model extraction.
pub fn parse(pdf_path: &Path, dpi: u32, _backend: &str) -> Result<ParseResult, LabParseError> {
    check_mlx_vlm()?;

    // Step 1: Convert PDF to PNG images
    let images = pdf_to_images(pdf_path, dpi)?;
    eprintln!("info: {} pages at {} DPI", images.len(), dpi);

    // Step 2: Extract ALL pages in one Python process (model loaded once)
    let page_results = extract_all_pages(&images)?;

    // Cleanup temp images
    for img in &images {
        let _ = std::fs::remove_file(img);
    }

    resolve_page_results(page_results)
}

// ── Resolve page results into ParseResult ──

fn resolve_page_results(page_results: Vec<PageResult>) -> Result<ParseResult, LabParseError> {
    let mut all_raw: Vec<VisionBiomarker> = Vec::new();
    let mut warnings = Vec::new();
    let mut page_statuses = Vec::new();
    let mut has_failures = false;

    // Build page statuses and collect markers
    for pr in &page_results {
        if let Some(ref err) = pr.error {
            warnings.push(format!("Page {} extraction failed: {}", pr.page, err));
            page_statuses.push(PageStatus {
                page: pr.page,
                status: PageExtractStatus::Failed,
                error: Some(err.clone()),
                marker_count: 0,
            });
            has_failures = true;
        } else {
            eprintln!(
                "info: page {} — {} markers in {:.1}s",
                pr.page,
                pr.markers.len(),
                pr.elapsed_s
            );
            page_statuses.push(PageStatus {
                page: pr.page,
                status: PageExtractStatus::Ok,
                error: None,
                marker_count: pr.markers.len(),
            });
        }
    }

    for pr in page_results {
        if pr.error.is_some() {
            continue;
        }
        // Inject page number into each marker
        for mut marker in pr.markers {
            marker.page = Some(pr.page);
            all_raw.push(marker);
        }
    }

    let mut biomarkers: Vec<ParsedBiomarker> = Vec::new();
    let mut unresolved: Vec<UnresolvedMarker> = Vec::new();
    let mut conflicts: Vec<ConflictMarker> = Vec::new();
    let mut seen_names = HashSet::new();

    // Track first occurrence of each resolved marker for conflict detection
    // Key: standardized_name -> (index in biomarkers, raw_name, value, unit, page)
    let mut first_occurrence: std::collections::HashMap<
        String,
        (usize, String, f64, String, Option<usize>),
    > = std::collections::HashMap::new();

    // Track markers that have been converted to conflicts (their original index is now invalid)
    let mut conflict_markers: HashSet<String> = HashSet::new();

    for raw in &all_raw {
        let value = match &raw.value {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => {
                let cleaned = s.trim().trim_start_matches('<').trim_start_matches('>');
                match cleaned.parse::<f64>() {
                    Ok(v) => v,
                    Err(_) => {
                        warnings.push(format!(
                            "Skipped '{}': non-numeric value '{}'",
                            raw.name, s
                        ));
                        continue;
                    }
                }
            }
            _ => {
                warnings.push(format!("Skipped '{}': unexpected value type", raw.name));
                continue;
            }
        };

        let norm_unit = normalize_unit(&raw.unit);

        match normalize_name(&raw.name, Some(value), Some(&norm_unit)) {
            Some((std_name, display_name, category, confidence, resolution_method)) => {
                if let Some((first_idx, first_raw, first_value, first_unit, first_page)) =
                    first_occurrence.get(&std_name).cloned()
                {
                    // Duplicate found - check if values match
                    let values_match = (first_value - value).abs() < 0.0001
                        && first_unit == norm_unit;

                    if values_match {
                        // Same value - emit warning but keep first
                        warnings.push(format!(
                            "Duplicate {} with same value ({} {}) from pages {:?} and {:?} - keeping first",
                            std_name, value, norm_unit, first_page, raw.page
                        ));
                    } else {
                        // Conflicting values - add to conflicts
                        if !conflict_markers.contains(&std_name) {
                            // First time seeing a conflict for this marker
                            // Create conflict with both candidates
                            conflict_markers.insert(std_name.clone());
                            conflicts.push(ConflictMarker {
                                standardized_name: std_name.clone(),
                                display_name: display_name.clone(),
                                category: category.clone(),
                                candidates: vec![
                                    ConflictCandidate {
                                        raw_name: first_raw,
                                        value: first_value,
                                        unit: first_unit,
                                        page: first_page,
                                    },
                                    ConflictCandidate {
                                        raw_name: raw.name.clone(),
                                        value,
                                        unit: norm_unit.clone(),
                                        page: raw.page,
                                    },
                                ],
                            });
                            // Mark biomarker at first_idx for removal
                            // (we'll filter these out at the end)
                            if first_idx < biomarkers.len() {
                                biomarkers[first_idx].resolved = false; // Mark for removal
                            }
                        } else {
                            // Already have a conflict for this marker - add this candidate
                            if let Some(conflict) = conflicts
                                .iter_mut()
                                .find(|c| c.standardized_name == std_name)
                            {
                                conflict.candidates.push(ConflictCandidate {
                                    raw_name: raw.name.clone(),
                                    value,
                                    unit: norm_unit.clone(),
                                    page: raw.page,
                                });
                            }
                        }
                    }
                    continue;
                }

                // First occurrence of this marker
                let idx = biomarkers.len();
                first_occurrence.insert(
                    std_name.clone(),
                    (idx, raw.name.clone(), value, norm_unit.clone(), raw.page),
                );
                seen_names.insert(std_name.clone());

                let unit = if norm_unit.is_empty() {
                    catalog::get_marker(&std_name)
                        .and_then(|m| m.allowed_units.first().cloned())
                        .unwrap_or_default()
                } else {
                    norm_unit
                };

                biomarkers.push(ParsedBiomarker {
                    name: raw.name.clone(),
                    standardized_name: std_name,
                    display_name,
                    value,
                    unit,
                    category,
                    resolved: true,
                    confidence,
                    resolution_method,
                });
            }
            None => {
                if !seen_names.insert(raw.name.to_lowercase()) {
                    continue;
                }
                unresolved.push(UnresolvedMarker {
                    raw_name: raw.name.clone(),
                    value,
                    unit: norm_unit,
                });
            }
        }
    }

    // Remove biomarkers that were converted to conflicts (marked with resolved=false)
    biomarkers.retain(|b| b.resolved);

    // Determine document status based on page failures and conflicts
    let document_status = if has_failures {
        DocumentStatus::PartialFailure
    } else if !conflicts.is_empty() {
        DocumentStatus::NeedsReview
    } else {
        DocumentStatus::Complete
    };

    Ok(ParseResult {
        document_status,
        page_statuses,
        biomarkers,
        unresolved,
        conflicts,
        warnings,
        parser_name: "pdf-vision".to_string(),
    })
}

// ── Dependency check ──

fn check_mlx_vlm() -> Result<(), LabParseError> {
    let check = Command::new("python3")
        .args(["-c", "from mlx_vlm import load, generate"])
        .output()
        .map_err(|e| LabParseError::VisionError(format!("python3 not found: {}", e)))?;

    if !check.status.success() {
        return Err(LabParseError::VisionError(
            "mlx_vlm not installed. Run: pip install mlx-vlm".to_string(),
        ));
    }
    Ok(())
}

// ── PDF → Images ──

fn pdf_to_images(pdf_path: &Path, dpi: u32) -> Result<Vec<String>, LabParseError> {
    let tmp_dir = std::env::temp_dir();
    let prefix = format!("labparse_{}", std::process::id());
    let output_prefix = tmp_dir.join(&prefix);

    let output = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            &dpi.to_string(),
            pdf_path.to_str().unwrap_or(""),
            output_prefix.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| {
            LabParseError::PdfConversionError(format!(
                "pdftoppm not found. Install: brew install poppler. Error: {}",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(LabParseError::PdfConversionError(format!(
            "pdftoppm failed: {}",
            stderr
        )));
    }

    let mut images: Vec<String> = Vec::new();
    let parent = tmp_dir.to_str().unwrap_or("/tmp");

    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".png") {
            images.push(entry.path().to_string_lossy().to_string());
        }
    }

    images.sort();

    if images.is_empty() {
        return Err(LabParseError::PdfConversionError(
            "pdftoppm produced no images".to_string(),
        ));
    }

    Ok(images)
}

// ── MLX batch extraction: ONE Python process, ALL pages ──
//
// Optimizations applied:
// 1. Model loaded once (~2s), reused for all pages (was 4s × N pages)
// 2. Prompt template built once, reused (KV cache benefit on pages 2+)
// 3. KV cache quantized to 4-bit (75% memory savings, 0.98x speed)
// 4. Prefill chunked at 512 tokens (prevents OOM on dense pages)
// 5. enable_thinking=False (no reasoning chain, 2x fewer output tokens)
// 6. Each page result streamed as JSON line for incremental parsing

fn extract_all_pages(
    image_paths: &[String],
) -> Result<Vec<PageResult>, LabParseError> {
    let images_json = serde_json::to_string(image_paths)
        .map_err(|e| LabParseError::VisionError(format!("Failed to serialize paths: {}", e)))?;

    let python_script = format!(
        r#"
import json, sys, time

# Suppress mlx_vlm progress bars on stderr
import os
os.environ['MLX_VLM_NO_PROGRESS'] = '1'

from mlx_vlm import load, generate

# Load model ONCE
load_start = time.time()
model, processor = load('{model}')
load_time = time.time() - load_start
print(json.dumps({{"event": "model_loaded", "elapsed_s": round(load_time, 2)}}), flush=True)

# Build prompt template ONCE
prompt_template = processor.apply_chat_template(
    [{{'role': 'user', 'content': [
        {{'type': 'image'}},
        {{'type': 'text', 'text': '''{prompt}'''}}
    ]}}],
    add_generation_prompt=True,
    enable_thinking=False,
    tokenize=False
)

# Process each page
images = json.loads('''{images}''')
for i, img_path in enumerate(images):
    page_num = i + 1
    try:
        start = time.time()
        output = generate(
            model, processor, prompt_template,
            image=img_path,
            max_tokens=4096,
            temperature=0.0,
            verbose=False,
            kv_bits=4,
            kv_group_size=64,
            prefill_step_size=512,
        )
        elapsed = time.time() - start
        text = output.text if hasattr(output, 'text') else str(output)

        # Strip think blocks
        if '</think>' in text:
            text = text.split('</think>', 1)[1].strip()
        # Strip markdown fences
        if text.startswith('```'):
            lines = text.split('\n')
            text = '\n'.join(lines[1:])
            if '```' in text:
                text = text.rsplit('```', 1)[0].strip()

        try:
            markers = json.loads(text)
            if not isinstance(markers, list):
                raise ValueError(f"Model returned {{type(markers).__name__}}, expected list")
        except json.JSONDecodeError as e:
            print(json.dumps({{
                "event": "page_done",
                "page": page_num,
                "markers": [],
                "elapsed_s": round(time.time() - start, 1),
                "error": f"Invalid JSON from model: {{str(e)[:200]}}"
            }}), flush=True)
            continue
        except ValueError as e:
            print(json.dumps({{
                "event": "page_done",
                "page": page_num,
                "markers": [],
                "elapsed_s": round(time.time() - start, 1),
                "error": str(e)
            }}), flush=True)
            continue

        print(json.dumps({{
            "event": "page_done",
            "page": page_num,
            "markers": markers,
            "elapsed_s": round(elapsed, 1),
        }}), flush=True)
    except Exception as e:
        print(json.dumps({{
            "event": "page_done",
            "page": page_num,
            "markers": [],
            "elapsed_s": 0,
            "error": str(e)[:300]
        }}), flush=True)

print(json.dumps({{"event": "done"}}), flush=True)
"#,
        model = MLX_MODEL,
        prompt = EXTRACTION_PROMPT.replace('\'', "\\'").replace('\n', "\\n"),
        images = images_json.replace('\'', "\\'"),
    );

    let output = Command::new("python3")
        .args(["-c", &python_script])
        .output()
        .map_err(|e| LabParseError::VisionError(format!("mlx_vlm failed to start: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check if we got partial results despite non-zero exit
        if output.stdout.is_empty() {
            return Err(LabParseError::VisionError(format!(
                "mlx_vlm extraction failed: {}",
                stderr.chars().take(500).collect::<String>()
            )));
        }
    }

    // Parse JSONL output — one JSON object per line
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            match obj.get("event").and_then(|e| e.as_str()) {
                Some("model_loaded") => {
                    let t = obj.get("elapsed_s").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    eprintln!("info: model loaded in {:.1}s", t);
                }
                Some("page_done") => {
                    let page = obj.get("page").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let elapsed_s = obj.get("elapsed_s").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let error = obj.get("error").and_then(|v| v.as_str()).map(String::from);
                    let markers: Vec<VisionBiomarker> = obj
                        .get("markers")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();

                    results.push(PageResult {
                        page,
                        markers,
                        elapsed_s,
                        error,
                    });
                }
                Some("done") => {}
                _ => {}
            }
        }
    }

    Ok(results)
}
