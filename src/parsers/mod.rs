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
