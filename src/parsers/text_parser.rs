use once_cell::sync::Lazy;
use regex::Regex;

use crate::errors::LabParseError;
use crate::normalize::{normalize_name, normalize_unit, ParsedBiomarker};
use crate::parsers::ParseResult;

/// Pattern: <name> <value> <unit>
/// Examples:
///   "HbA1c 5.8%"
///   "ApoB 95 mg/dL"
///   "LDL 130 mg/dL"
///   "Fasting Glucose 92 mg/dL"
///   "eGFR 95 mL/min/1.73m²"
///   "Vitamin D 45 ng/mL"
///   "Free T4 1.2 ng/dL"
static BIOMARKER_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?xi)
        # Biomarker name: letters, Greek, hyphens, spaces, parens, slashes
        # The first word can have digits (e.g., HbA1c), but subsequent ones should not
        (?P<name>[A-Za-zα-ωΑ-Ω0-9\-\(\)/]+(?:\s+[A-Za-zα-ωΑ-Ω\-\(\)/]+){0,4})
        # Separator (colon, equals, or whitespace)
        [\s:=]+
        # Optional < or >
        [<>]?
        # Numeric value (with optional decimal)
        (?P<value>\d+(?:\.\d+)?)
        # Optional whitespace
        \s*
        # Unit (optional, various patterns)
        (?P<unit>
            %
            | ratio
            | score
            | [a-zA-Zµ°/²³]+(?:/[a-zA-Zµ°²³\d.]+)*
        )?
        "
    )
    .unwrap()
});

/// Pattern for "name: value unit" with colon separator
static COLON_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?xi)
        (?P<name>[A-Za-z0-9\-\(\)/\s]{2,40})
        \s*:\s*
        [<>]?
        (?P<value>\d+(?:\.\d+)?)
        \s*
        (?P<unit>[a-zA-Zµ°/%²³]+(?:/[a-zA-Zµ°²³\d.]+)*)?
        "
    )
    .unwrap()
});

pub fn parse(content: &str, _source: &str) -> Result<ParseResult, LabParseError> {
    let mut biomarkers = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    let mut matched_spans = Vec::new();

    // Try COLON_PATTERN first as it's more specific
    for cap in COLON_PATTERN.captures_iter(content) {
        let mat = cap.get(0).unwrap();
        let span = (mat.start(), mat.end());
        
        if let Some(bm) = try_extract(&cap, &mut seen_names, &mut warnings) {
            biomarkers.push(bm);
            matched_spans.push(span);
        }
    }

    // Try general pattern for segments not already matched
    for cap in BIOMARKER_PATTERN.captures_iter(content) {
        let mat = cap.get(0).unwrap();
        let start = mat.start();
        let end = mat.end();
        
        // Check if this match overlaps with any COLON_PATTERN match
        let is_overlapping = matched_spans.iter().any(|(ms, me)| {
            (start >= *ms && start < *me) || (end > *ms && end <= *me)
        });

        if !is_overlapping {
            if let Some(bm) = try_extract(&cap, &mut seen_names, &mut warnings) {
                biomarkers.push(bm);
            }
        }
    }

    Ok(ParseResult {
        biomarkers,
        warnings,
        parser_name: "text".to_string(),
    })
}

fn try_extract(
    cap: &regex::Captures,
    seen_names: &mut std::collections::HashSet<String>,
    warnings: &mut Vec<String>,
) -> Option<ParsedBiomarker> {
    let raw_name = cap.name("name")?.as_str().trim();
    let raw_value = cap.name("value")?.as_str();
    let raw_unit = cap.name("unit").map(|m| m.as_str()).unwrap_or("");

    let value: f64 = raw_value.parse().ok()?;

    let (std_name, display_name, category) = normalize_name(raw_name)?;

    // Skip duplicates
    if !seen_names.insert(std_name.to_string()) {
        warnings.push(format!("Duplicate result for {} skipped: '{}'", std_name, raw_name));
        return None;
    }

    let unit = if raw_unit.is_empty() {
        crate::biomarkers::get_definition(std_name)
            .map(|d| d.standard_unit.clone())
            .unwrap_or_default()
    } else {
        normalize_unit(raw_unit)
    };

    Some(ParsedBiomarker {
        name: raw_name.to_string(),
        standardized_name: std_name.to_string(),
        display_name,
        value,
        unit,
        category,
    })
}
