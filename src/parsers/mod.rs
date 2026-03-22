pub mod csv_parser;
pub mod text_parser;

use crate::errors::LabParseError;
use crate::normalize::ParsedBiomarker;

/// Result of parsing a lab document
#[derive(Debug)]
pub struct ParseResult {
    pub biomarkers: Vec<ParsedBiomarker>,
    pub warnings: Vec<String>,
    pub parser_name: String,
}

/// Detect whether content looks like CSV (structured tabular data)
pub fn looks_like_csv(content: &str) -> bool {
    let lines: Vec<&str> = content.lines().take(5).collect();
    if lines.len() < 2 {
        // Need at least header + 1 data row
        return false;
    }
    let first = lines[0];
    let comma_count = first.matches(',').count();
    if comma_count < 2 {
        return false;
    }
    // Check that multiple lines have a consistent number of commas (tabular structure)
    let second_commas = lines[1].matches(',').count();
    // Also check that the first line looks like headers (no digits in most fields)
    let fields: Vec<&str> = first.split(',').collect();
    let alpha_fields = fields
        .iter()
        .filter(|f| {
            let trimmed = f.trim();
            !trimmed.is_empty() && trimmed.chars().all(|c| c.is_alphabetic() || c.is_whitespace())
        })
        .count();
    // At least half the fields should be alpha-only (headers) and row counts should be similar
    comma_count >= 2 && alpha_fields >= fields.len() / 2 && (second_commas as i32 - comma_count as i32).abs() <= 1
}

/// Auto-detect and parse content
pub fn auto_parse(content: &str, source: &str) -> Result<ParseResult, LabParseError> {
    if looks_like_csv(content) {
        csv_parser::parse(content, source)
    } else {
        text_parser::parse(content, source)
    }
}
