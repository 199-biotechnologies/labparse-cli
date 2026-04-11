pub mod csv_parser;
pub mod pdf_parser;
pub mod text_parser;

use crate::errors::LabParseError;
use crate::normalize::ParsedBiomarker;

/// An unresolved marker — structured passthrough
#[derive(Debug, Clone)]
pub struct UnresolvedMarker {
    pub raw_name: String,
    pub value: f64,
    pub unit: String,
}

/// A candidate value when multiple values exist for the same marker
#[derive(Debug, Clone)]
pub struct ConflictCandidate {
    pub raw_name: String,
    pub value: f64,
    pub unit: String,
    pub page: Option<usize>,
}

/// A conflict where multiple values exist for the same marker
#[derive(Debug, Clone)]
pub struct ConflictMarker {
    pub standardized_name: String,
    pub display_name: String,
    pub category: String,
    pub candidates: Vec<ConflictCandidate>,
}

/// Document-level extraction status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStatus {
    Complete,
    PartialFailure, // Some pages failed
    NeedsReview,    // Conflicts or ambiguities
}

/// Per-page extraction status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Partial variant used in future validation/escalation
pub enum PageExtractStatus {
    Ok,
    Failed,
    Partial, // Some markers extracted but issues
}

/// Per-page extraction result
#[derive(Debug, Clone)]
pub struct PageStatus {
    pub page: usize,
    pub status: PageExtractStatus,
    pub error: Option<String>,
    pub marker_count: usize,
}

/// Result of parsing a lab document
#[derive(Debug)]
pub struct ParseResult {
    pub document_status: DocumentStatus,
    pub page_statuses: Vec<PageStatus>,
    pub biomarkers: Vec<ParsedBiomarker>,
    pub unresolved: Vec<UnresolvedMarker>,
    pub conflicts: Vec<ConflictMarker>,
    pub warnings: Vec<String>,
    pub parser_name: String,
    pub lexical_rejections: usize,
}

/// Detect whether content looks like CSV (structured tabular data)
pub fn looks_like_csv(content: &str) -> bool {
    // Strip BOM for detection
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    let lines: Vec<&str> = content.lines().take(5).collect();
    if lines.len() < 2 {
        // Need at least header + 1 data row
        return false;
    }
    let first = lines[0];

    // Check multiple delimiters: comma, tab, semicolon
    let comma_count = first.matches(',').count();
    let tab_count = first.matches('\t').count();
    let semi_count = first.matches(';').count();

    // Pick the best delimiter
    let (delim_count, delim_char) = if tab_count >= 2 && tab_count >= comma_count && tab_count >= semi_count {
        (tab_count, '\t')
    } else if semi_count >= 2 && semi_count >= comma_count {
        (semi_count, ';')
    } else {
        (comma_count, ',')
    };

    if delim_count < 2 {
        return false;
    }

    // Check that multiple lines have a consistent delimiter count (tabular structure)
    let second_count = lines[1].matches(delim_char).count();
    // Also check that the first line looks like headers (no digits in most fields)
    let fields: Vec<&str> = first.split(delim_char).collect();
    let alpha_fields = fields
        .iter()
        .filter(|f| {
            let trimmed = f.trim();
            !trimmed.is_empty() && trimmed.chars().all(|c| c.is_alphabetic() || c.is_whitespace())
        })
        .count();
    // At least half the fields should be alpha-only (headers) and row counts should be similar
    delim_count >= 2 && alpha_fields >= fields.len() / 2 && (second_count as i32 - delim_count as i32).abs() <= 1
}

/// Auto-detect and parse content
pub fn auto_parse(content: &str, source: &str) -> Result<ParseResult, LabParseError> {
    if looks_like_csv(content) {
        csv_parser::parse(content, source)
    } else {
        text_parser::parse(content, source)
    }
}

/// Detect duplicate biomarkers with different values and emit them as conflicts.
/// Same standardized_name with same value+unit+comparator → keep first.
/// Same standardized_name with different values → flag as conflict.
/// Preserves original input order for determinism.
pub fn detect_conflicts(
    biomarkers: Vec<ParsedBiomarker>,
    warnings: &mut Vec<String>,
) -> (Vec<ParsedBiomarker>, Vec<ConflictMarker>) {
    use std::collections::HashMap;

    // Build groups but track first-seen order for determinism
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<ParsedBiomarker>> = HashMap::new();
    for bm in biomarkers {
        let key = bm.standardized_name.clone();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(bm);
    }

    let mut kept = Vec::new();
    let mut conflicts = Vec::new();

    for std_name in order {
        let group = groups.remove(&std_name).unwrap();
        if group.len() == 1 {
            kept.push(group.into_iter().next().unwrap());
            continue;
        }

        let first = &group[0];
        let all_match = group.iter().all(|b| {
            (b.value - first.value).abs() < 0.0001
                && b.unit == first.unit
                && b.comparator == first.comparator
        });

        if all_match {
            warnings.push(format!(
                "Duplicate {} with same value ({} {}) — keeping first",
                std_name, first.value, first.unit
            ));
            kept.push(group.into_iter().next().unwrap());
        } else {
            warnings.push(format!(
                "Conflict: {} has {} different values — flagged for review",
                std_name, group.len()
            ));
            let candidates: Vec<ConflictCandidate> = group.iter().map(|b| ConflictCandidate {
                raw_name: b.name.clone(),
                value: b.value,
                unit: b.unit.clone(),
                page: None,
            }).collect();
            conflicts.push(ConflictMarker {
                standardized_name: std_name,
                display_name: first.display_name.clone(),
                category: first.category.clone(),
                candidates,
            });
        }
    }

    (kept, conflicts)
}
