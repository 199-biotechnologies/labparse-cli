use serde::Serialize;

use crate::normalize::{Comparator, ParsedBiomarker, UnitStatus};
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

/// Helper to check if comparator is Eq (for skip_serializing_if)
fn is_eq_comparator(cmp: &Option<Comparator>) -> bool {
    cmp.map_or(true, |c| c.is_eq())
}

/// Helper to check if flagged is false (for skip_serializing_if)
fn is_false(v: &bool) -> bool {
    !*v
}

/// Helper to check if unit_status is Observed (for skip_serializing_if)
fn is_observed_unit(status: &Option<UnitStatus>) -> bool {
    status.map_or(true, |s| s.is_observed())
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
    /// Value comparator (<, >, <=, >=) - omitted when Eq (exact value)
    #[serde(skip_serializing_if = "is_eq_comparator")]
    pub comparator: Option<Comparator>,
    /// Reference range from the lab report (e.g., "4.0 - 5.5")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_range: Option<String>,
    /// Whether the value is flagged as abnormal (outside reference range)
    #[serde(skip_serializing_if = "is_false")]
    pub flagged: bool,
    /// Unit provenance: "Inferred" or "Missing" - omitted when "Observed" (from source)
    #[serde(skip_serializing_if = "is_observed_unit")]
    pub unit_status: Option<UnitStatus>,
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
        // Only include comparator if it's not Eq
        let comparator = if bm.comparator.is_eq() {
            None
        } else {
            Some(bm.comparator)
        };

        // Only include unit_status if it's not Observed
        let unit_status = if bm.unit_status.is_observed() {
            None
        } else {
            Some(bm.unit_status)
        };

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
            comparator,
            reference_range: bm.reference_range.clone(),
            flagged: bm.flagged,
            unit_status,
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
