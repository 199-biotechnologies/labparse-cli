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
    let (cmp, num_str) = if trimmed.starts_with("<=") {
        (Comparator::Le, &trimmed[2..])
    } else if trimmed.starts_with(">=") {
        (Comparator::Ge, &trimmed[2..])
    } else if trimmed.starts_with('≤') {
        (Comparator::Le, &trimmed['≤'.len_utf8()..])
    } else if trimmed.starts_with('≥') {
        (Comparator::Ge, &trimmed['≥'.len_utf8()..])
    } else if trimmed.starts_with('<') {
        (Comparator::Lt, &trimmed[1..])
    } else if trimmed.starts_with('>') {
        (Comparator::Gt, &trimmed[1..])
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
    let mut seen_names = std::collections::HashSet::new();

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
                if !seen_names.insert(std_name.clone()) {
                    warnings.push(format!("Duplicate result for {} skipped: '{}'", std_name, raw_name));
                    continue;
                }

                let (unit, unit_status) = if norm_unit.is_empty() {
                    // Only infer unit when marker has exactly 1 allowed unit
                    match catalog::get_marker(&std_name) {
                        Some(m) if m.allowed_units.len() == 1 => {
                            (m.allowed_units[0].clone(), UnitStatus::Inferred)
                        }
                        _ => (String::new(), UnitStatus::Missing),
                    }
                } else {
                    (norm_unit, UnitStatus::Observed)
                };

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

    let has_missing_units = biomarkers.iter().any(|b| b.unit_status == UnitStatus::Missing);
    let has_ambiguous = biomarkers.iter().any(|b| b.confidence == "ambiguous");
    let document_status = if biomarkers.is_empty() && !unresolved.is_empty() {
        DocumentStatus::NeedsReview
    } else if !biomarkers.is_empty() && unresolved.len() > biomarkers.len() * 3 {
        DocumentStatus::NeedsReview
    } else if has_missing_units || has_ambiguous {
        DocumentStatus::NeedsReview
    } else {
        DocumentStatus::Complete
    };

    Ok(ParseResult {
        document_status,
        page_statuses: vec![],
        biomarkers,
        unresolved,
        conflicts: vec![],
        warnings,
        parser_name: "csv".to_string(),
    })
}
