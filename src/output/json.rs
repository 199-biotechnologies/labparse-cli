use serde::Serialize;

use crate::normalize::ParsedBiomarker;
use crate::parsers::{ConflictCandidate, ConflictMarker, ParseResult};

#[derive(Serialize)]
pub struct JsonEnvelope {
    pub version: &'static str,
    pub status: &'static str,
    pub data: JsonData,
    pub metadata: JsonMetadata,
}

#[derive(Serialize)]
pub struct JsonData {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_detected: Option<String>,
    pub biomarkers: Vec<JsonBiomarker>,
    pub unresolved: Vec<JsonUnresolved>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<JsonConflict>,
    pub parse_warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct JsonBiomarker {
    pub name: String,
    pub standardized_name: String,
    pub display_name: String,
    pub value: f64,
    pub unit: String,
    pub category: String,
    pub resolved: bool,
    pub confidence: String,
    pub resolution_method: String,
}

/// Unresolved markers — structured passthrough (not raw text in standardized_name)
#[derive(Serialize)]
pub struct JsonUnresolved {
    pub raw_name: String,
    pub value: f64,
    pub unit: String,
}

/// A conflict where multiple values exist for the same marker
#[derive(Serialize)]
pub struct JsonConflict {
    pub standardized_name: String,
    pub display_name: String,
    pub category: String,
    pub candidates: Vec<JsonConflictCandidate>,
}

/// A candidate value in a conflict
#[derive(Serialize)]
pub struct JsonConflictCandidate {
    pub raw_name: String,
    pub value: f64,
    pub unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
}

impl From<&ConflictMarker> for JsonConflict {
    fn from(c: &ConflictMarker) -> Self {
        Self {
            standardized_name: c.standardized_name.clone(),
            display_name: c.display_name.clone(),
            category: c.category.clone(),
            candidates: c.candidates.iter().map(JsonConflictCandidate::from).collect(),
        }
    }
}

impl From<&ConflictCandidate> for JsonConflictCandidate {
    fn from(c: &ConflictCandidate) -> Self {
        Self {
            raw_name: c.raw_name.clone(),
            value: c.value,
            unit: c.unit.clone(),
            page: c.page,
        }
    }
}

#[derive(Serialize)]
pub struct JsonMetadata {
    pub elapsed_ms: u128,
    pub markers_found: usize,
    pub markers_unresolved: usize,
    pub parser: String,
    pub catalog_version: &'static str,
}

impl From<&ParsedBiomarker> for JsonBiomarker {
    fn from(bm: &ParsedBiomarker) -> Self {
        Self {
            name: bm.name.clone(),
            standardized_name: bm.standardized_name.clone(),
            display_name: bm.display_name.clone(),
            value: bm.value,
            unit: bm.unit.clone(),
            category: bm.category.clone(),
            resolved: bm.resolved,
            confidence: bm.confidence.clone(),
            resolution_method: bm.resolution_method.clone(),
        }
    }
}

pub fn render(
    result: &ParseResult,
    source: &str,
    elapsed_ms: u128,
) -> String {
    // Use "needs_review" status if there are conflicts
    let status = if result.conflicts.is_empty() {
        "success"
    } else {
        "needs_review"
    };

    let envelope = JsonEnvelope {
        version: "2",
        status,
        data: JsonData {
            source: source.to_string(),
            date_detected: None,
            biomarkers: result.biomarkers.iter().map(JsonBiomarker::from).collect(),
            unresolved: result.unresolved.iter().map(|u| JsonUnresolved {
                raw_name: u.raw_name.clone(),
                value: u.value,
                unit: u.unit.clone(),
            }).collect(),
            conflicts: result.conflicts.iter().map(JsonConflict::from).collect(),
            parse_warnings: result.warnings.clone(),
        },
        metadata: JsonMetadata {
            elapsed_ms,
            markers_found: result.biomarkers.len(),
            markers_unresolved: result.unresolved.len(),
            parser: result.parser_name.clone(),
            catalog_version: "2.0",
        },
    };
    serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
}

pub fn render_error(err: &str) -> String {
    serde_json::json!({
        "version": "2",
        "status": "error",
        "error": err
    })
    .to_string()
}
