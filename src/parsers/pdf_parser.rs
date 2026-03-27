//! PDF lab report extraction via Qwen3.5-9B on Apple Silicon (MLX).
//!
//! Pipeline: PDF → pdftoppm (150 DPI PNG) → mlx_vlm vision → JSON → catalog resolver
//!
//! Optimizations:
//! 1. 150 DPI rendering (4x fewer vision tokens, lab text still readable)
//! 2. enable_thinking=False (no reasoning chain, 2x fewer tokens)
//!
//! Requires: pdftoppm (brew install poppler), mlx-vlm (pip install mlx-vlm)

use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::catalog;
use crate::errors::LabParseError;
use crate::normalize::{normalize_name, normalize_unit, ParsedBiomarker};
use crate::parsers::{ParseResult, UnresolvedMarker};

const EXTRACTION_PROMPT: &str = "Extract ALL biomarkers from this lab report page. Output ONLY a valid JSON array.
Each object must have exactly these fields: name, value, unit, reference_range.
Rules:
- value must be a number (float or int), not a string
- unit must be the exact unit shown (e.g., mmol/L, g/L, IU/L, X10^9/L)
- reference_range must be the range shown (e.g., \"4.3 - 5.4\")
- If a value is flagged/highlighted as abnormal, add \"flagged\": true
- Do NOT include section headers (e.g., \"Kidney Function\", \"Liver Function\")
- Do NOT skip any biomarker row
Output ONLY the JSON array, nothing else.";

const MLX_MODEL: &str = "mlx-community/Qwen3.5-9B-4bit";

/// Raw biomarker from vision model output
#[derive(Debug, Deserialize)]
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
}

/// Parse a PDF file using MLX vision model extraction.
pub fn parse(pdf_path: &Path, dpi: u32, _backend: &str) -> Result<ParseResult, LabParseError> {
    // Verify mlx_vlm is available
    check_mlx_vlm()?;

    // Step 1: Convert PDF to PNG images
    let images = pdf_to_images(pdf_path, dpi)?;
    eprintln!("info: {} pages at {} DPI", images.len(), dpi);

    // Step 2: Extract biomarkers from each page via MLX
    let mut all_raw: Vec<VisionBiomarker> = Vec::new();
    let mut warnings = Vec::new();

    for (i, img_path) in images.iter().enumerate() {
        let start = std::time::Instant::now();

        match extract_via_mlx(img_path) {
            Ok(markers) => {
                let elapsed = start.elapsed().as_secs_f32();
                eprintln!(
                    "info: page {} — {} markers in {:.1}s",
                    i + 1,
                    markers.len(),
                    elapsed
                );
                all_raw.extend(markers);
            }
            Err(e) => {
                warnings.push(format!("Page {} extraction failed: {}", i + 1, e));
            }
        }
    }

    // Cleanup temp images
    for img in &images {
        let _ = std::fs::remove_file(img);
    }

    // Step 3: Deduplicate and resolve through catalog
    let mut biomarkers = Vec::new();
    let mut unresolved = Vec::new();
    let mut seen_names = HashSet::new();

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
                if !seen_names.insert(std_name.clone()) {
                    continue;
                }

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

    Ok(ParseResult {
        biomarkers,
        unresolved,
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

// ── MLX Vision extraction (Qwen3.5-9B via mlx_vlm) ──

fn extract_via_mlx(image_path: &str) -> Result<Vec<VisionBiomarker>, LabParseError> {
    let python_script = format!(
        r#"
import json
from mlx_vlm import load, generate

model, processor = load('{model}')
prompt = processor.apply_chat_template(
    [{{'role': 'user', 'content': [
        {{'type': 'image'}},
        {{'type': 'text', 'text': '''{prompt}'''}}
    ]}}],
    add_generation_prompt=True,
    enable_thinking=False,
    tokenize=False
)
output = generate(model, processor, prompt, image='{image}', max_tokens=4096, temperature=0.0, verbose=False)
text = output.text if hasattr(output, 'text') else str(output)
print(text)
"#,
        model = MLX_MODEL,
        prompt = EXTRACTION_PROMPT.replace('\'', "\\'"),
        image = image_path.replace('\'', "\\'"),
    );

    let output = Command::new("python3")
        .args(["-c", &python_script])
        .output()
        .map_err(|e| LabParseError::VisionError(format!("mlx_vlm failed to start: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(LabParseError::VisionError(format!(
            "mlx_vlm extraction failed: {}",
            stderr.chars().take(500).collect::<String>()
        )));
    }

    let content = String::from_utf8_lossy(&output.stdout);
    Ok(parse_vision_json(&content))
}

// ── JSON response parsing ──

fn parse_vision_json(content: &str) -> Vec<VisionBiomarker> {
    let mut text = content.trim().to_string();

    // Strip <think>...</think> blocks if model ignored enable_thinking=False
    if let Some(think_end) = text.find("</think>") {
        text = text[think_end + 8..].trim().to_string();
    }

    // Strip markdown code fences
    let json_str = if text.starts_with("```") {
        let inner = text
            .split('\n')
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n");
        inner
            .rsplit_once("```")
            .map(|(before, _)| before.trim())
            .unwrap_or(inner.trim())
            .to_string()
    } else {
        text
    };

    match serde_json::from_str::<Vec<VisionBiomarker>>(&json_str) {
        Ok(markers) => markers,
        Err(e) => {
            eprintln!("warn: failed to parse vision JSON: {}", e);
            eprintln!(
                "warn: raw content (first 200 chars): {}",
                &json_str[..json_str.len().min(200)]
            );
            Vec::new()
        }
    }
}
