use crate::catalog;
use crate::errors::LabParseError;
use crate::normalize::{normalize_name, normalize_unit, parse_number, Comparator, ParsedBiomarker, UnitStatus};
use crate::parsers::{DocumentStatus, ParseResult, UnresolvedMarker};

/// Common header names for the test/biomarker name column
const NAME_HEADERS: &[&str] = &[
    "test name",
    "test",
    "analyte",
    "component",
    "biomarker",
    "marker",
    "name",
    "assay",
    "description",
    "test description",
    "lab test",
    "observation",
];

/// Common header names for the result/value column
const VALUE_HEADERS: &[&str] = &[
    "result",
    "value",
    "result value",
    "observed value",
    "test result",
    "numeric result",
    "reported value",
    "level",
];

/// Common header names for the unit column
const UNIT_HEADERS: &[&str] = &[
    "unit",
    "units",
    "uom",
    "unit of measure",
    "reference unit",
    "result unit",
];

fn find_column(headers: &csv::StringRecord, candidates: &[&str]) -> Option<usize> {
    for (i, h) in headers.iter().enumerate() {
        let lower = h.to_lowercase().trim().to_string();
        if candidates.contains(&lower.as_str()) {
            return Some(i);
        }
    }
    None
}

/// Parse a value string, extracting both the numeric value and any comparator prefix.
/// Returns (value, comparator).
fn parse_number_with_comparator(s: &str) -> Option<(f64, Comparator)> {
    let trimmed = s.trim();

    // Extract comparator and remaining numeric string (char-boundary safe)
    let (cmp, num_str) = if let Some(rest) = trimmed.strip_prefix("<=") {
        (Comparator::Le, rest)
    } else if let Some(rest) = trimmed.strip_prefix(">=") {
        (Comparator::Ge, rest)
    } else if let Some(rest) = trimmed.strip_prefix('≤') {
        (Comparator::Le, rest)
    } else if let Some(rest) = trimmed.strip_prefix('≥') {
        (Comparator::Ge, rest)
    } else if let Some(rest) = trimmed.strip_prefix('<') {
        (Comparator::Lt, rest)
    } else if let Some(rest) = trimmed.strip_prefix('>') {
        (Comparator::Gt, rest)
    } else {
        (Comparator::Eq, trimmed)
    };

    parse_number(num_str.trim()).ok().map(|parsed| (parsed.value, cmp))
}

/// Detect delimiter from the first line: tab, semicolon, or comma (default)
fn detect_delimiter(content: &str) -> u8 {
    let first_line = content.lines().next().unwrap_or("");
    let tabs = first_line.matches('\t').count();
    let semis = first_line.matches(';').count();
    let commas = first_line.matches(',').count();

    if tabs >= 2 && tabs >= semis && tabs >= commas {
        b'\t'
    } else if semis >= 2 && semis >= commas {
        b';'
    } else {
        b','
    }
}

pub fn parse(content: &str, _source: &str) -> Result<ParseResult, LabParseError> {
    // Strip UTF-8 BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    // Auto-detect delimiter from first line
    let delimiter = detect_delimiter(content);

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .delimiter(delimiter)
        .from_reader(content.as_bytes());

    let headers = rdr.headers()?.clone();
    let name_col = find_column(&headers, NAME_HEADERS);
    let value_col = find_column(&headers, VALUE_HEADERS);
    let unit_col = find_column(&headers, UNIT_HEADERS);

    let name_col = name_col.ok_or_else(|| {
        LabParseError::ParseFailure(
            "Could not find a test name column. Expected headers like: Test Name, Analyte, Component".to_string(),
        )
    })?;

    let value_col = value_col.ok_or_else(|| {
        LabParseError::ParseFailure(
            "Could not find a result/value column. Expected headers like: Result, Value".to_string(),
        )
    })?;

    let mut biomarkers = Vec::new();
    let mut unresolved = Vec::new();
    let mut warnings = Vec::new();
    let seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for record in rdr.records() {
        let record = record?;
        let raw_name = record.get(name_col).unwrap_or("").trim();
        let raw_value = record.get(value_col).unwrap_or("").trim();
        let raw_unit = unit_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .trim();

        if raw_name.is_empty() || raw_value.is_empty() {
            continue;
        }

        let (value, comparator) = match parse_number_with_comparator(raw_value) {
            Some((v, cmp)) => (v, cmp),
            None => {
                warnings.push(format!(
                    "Skipped '{}': non-numeric value '{}'",
                    raw_name, raw_value
                ));
                continue;
            }
        };

        let norm_unit = if raw_unit.is_empty() {
            String::new()
        } else {
            normalize_unit(raw_unit)
        };

        match normalize_name(raw_name, Some(value), Some(&norm_unit)) {
            Some((std_name, display_name, category, confidence, resolution_method)) => {
                // Don't skip duplicates here — let detect_conflicts handle them later
                let _ = seen_names;

                let (unit, unit_status) = if norm_unit.is_empty() {
                    match catalog::get_marker(&std_name) {
                        Some(m) if m.allowed_units.iter().any(|u| u.is_empty()) => {
                            (String::new(), UnitStatus::Observed)
                        }
                        Some(m) if m.allowed_units.len() == 1 => {
                            (m.allowed_units[0].clone(), UnitStatus::Inferred)
                        }
                        _ => (String::new(), UnitStatus::Missing),
                    }
                } else {
                    (norm_unit, UnitStatus::Observed)
                };

                let raw_unit_clone = raw_unit.to_string();
                biomarkers.push(ParsedBiomarker {
                    name: raw_name.to_string(),
                    standardized_name: std_name,
                    display_name,
                    value,
                    unit,
                    category,
                    resolved: true,
                    confidence,
                    resolution_method,
                    comparator,
                    reference_range: None,
                    flagged: false,
                    unit_status,
                    page: None,
                    raw_value_text: Some(raw_value.to_string()),
                    raw_unit: if raw_unit_clone.is_empty() { None } else { Some(raw_unit_clone) },
                    source_text: None,
                });
            }
            None => {
                // Structured passthrough — NOT raw text in standardized_name
                unresolved.push(UnresolvedMarker {
                    raw_name: raw_name.to_string(),
                    value,
                    unit: norm_unit,
                });
                warnings.push(format!("Unresolved biomarker: '{}'", raw_name));
            }
        }
    }

    // Detect conflicts: same standardized_name with different values
    let (biomarkers, conflicts) = crate::parsers::detect_conflicts(biomarkers, &mut warnings);

    let has_missing_units = biomarkers.iter().any(|b| b.unit_status == UnitStatus::Missing);
    let has_ambiguous = biomarkers.iter().any(|b| b.confidence == "ambiguous");
    let needs_review = (biomarkers.is_empty() && !unresolved.is_empty())
        || (!biomarkers.is_empty() && unresolved.len() > biomarkers.len() * 3)
        || has_missing_units
        || has_ambiguous
        || !conflicts.is_empty();
    let document_status = if needs_review {
        DocumentStatus::NeedsReview
    } else {
        DocumentStatus::Complete
    };

    Ok(ParseResult {
        document_status,
        page_statuses: vec![],
        biomarkers,
        unresolved,
        conflicts,
        warnings,
        parser_name: "csv".to_string(),
        lexical_rejections: 0,
    })
}
