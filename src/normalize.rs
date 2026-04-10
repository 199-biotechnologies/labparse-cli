use crate::catalog;

// ============================================================================
// Locale-aware number parsing
// ============================================================================

/// Result of parsing a numeric measurement
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedNumber {
    /// Original string before parsing
    pub raw: String,
    /// Parsed numeric value
    pub value: f64,
    /// True if parse was ambiguous (e.g., could be 1.234 or 1234)
    pub ambiguous: bool,
    /// How the number was interpreted: "decimal_dot", "decimal_comma", "thousands_comma", "plain"
    pub parse_strategy: String,
}

/// Parse a numeric value with locale awareness
///
/// Handles:
/// - Decimal dot (US/UK): 1.5, 1,234.5
/// - Decimal comma (EU): 1,5
/// - Thousands separators: 1,234 or 1.234
///
/// # Arguments
/// * `raw` - The raw string to parse
///
/// # Returns
/// * `Ok(ParsedNumber)` - Successfully parsed number with metadata
/// * `Err(String)` - Parse failure with reason
pub fn parse_number(raw: &str) -> Result<ParsedNumber, String> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err("Empty input".to_string());
    }

    // Strip leading comparators (<, >, <=, >=) if present
    let numeric_part = trimmed
        .trim_start_matches("<=")
        .trim_start_matches(">=")
        .trim_start_matches('<')
        .trim_start_matches('>')
        .trim_start_matches('≤')
        .trim_start_matches('≥')
        .trim();

    if numeric_part.is_empty() {
        return Err("No numeric content after comparator".to_string());
    }

    let has_comma = numeric_part.contains(',');
    let has_dot = numeric_part.contains('.');

    // Case 1: Both dot and comma present - last separator is decimal
    if has_comma && has_dot {
        let last_comma = numeric_part.rfind(',').unwrap();
        let last_dot = numeric_part.rfind('.').unwrap();

        if last_comma > last_dot {
            // Format: 1.234,56 (EU with thousands dot)
            let normalized = numeric_part.replace('.', "").replace(',', ".");
            match normalized.parse::<f64>() {
                Ok(v) => Ok(ParsedNumber {
                    raw: raw.to_string(),
                    value: v,
                    ambiguous: false,
                    parse_strategy: "decimal_comma".to_string(),
                }),
                Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
            }
        } else {
            // Format: 1,234.56 (US with thousands comma)
            let normalized = numeric_part.replace(',', "");
            match normalized.parse::<f64>() {
                Ok(v) => Ok(ParsedNumber {
                    raw: raw.to_string(),
                    value: v,
                    ambiguous: false,
                    parse_strategy: "decimal_dot".to_string(),
                }),
                Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
            }
        }
    }
    // Case 2: Only comma present
    else if has_comma {
        let parts: Vec<&str> = numeric_part.split(',').collect();

        if parts.len() == 2 {
            let after_comma = parts[1];
            let after_len = after_comma.len();

            // 1-2 digits after comma: decimal comma (1,5 -> 1.5, 1,25 -> 1.25)
            if after_len >= 1 && after_len <= 2 {
                let normalized = numeric_part.replace(',', ".");
                match normalized.parse::<f64>() {
                    Ok(v) => Ok(ParsedNumber {
                        raw: raw.to_string(),
                        value: v,
                        ambiguous: false,
                        parse_strategy: "decimal_comma".to_string(),
                    }),
                    Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
                }
            }
            // Exactly 3 digits after comma: thousands separator (1,234 -> 1234)
            else if after_len == 3 && after_comma.chars().all(|c| c.is_ascii_digit()) {
                let normalized = numeric_part.replace(',', "");
                match normalized.parse::<f64>() {
                    Ok(v) => Ok(ParsedNumber {
                        raw: raw.to_string(),
                        value: v,
                        ambiguous: true, // Could be EU decimal 1,234 = 1.234
                        parse_strategy: "thousands_comma".to_string(),
                    }),
                    Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
                }
            } else {
                // Unusual pattern - try decimal comma as fallback
                let normalized = numeric_part.replace(',', ".");
                match normalized.parse::<f64>() {
                    Ok(v) => Ok(ParsedNumber {
                        raw: raw.to_string(),
                        value: v,
                        ambiguous: true,
                        parse_strategy: "decimal_comma".to_string(),
                    }),
                    Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
                }
            }
        } else {
            // Multiple commas: assume thousands separators (1,234,567)
            let normalized = numeric_part.replace(',', "");
            match normalized.parse::<f64>() {
                Ok(v) => Ok(ParsedNumber {
                    raw: raw.to_string(),
                    value: v,
                    ambiguous: false,
                    parse_strategy: "thousands_comma".to_string(),
                }),
                Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
            }
        }
    }
    // Case 3: Only dot present
    else if has_dot {
        let parts: Vec<&str> = numeric_part.split('.').collect();

        if parts.len() == 2 {
            let after_dot = parts[1];

            // Exactly 3 digits after dot could be EU thousands separator (1.234 = 1234)
            // But we prefer decimal interpretation - mark as ambiguous
            if after_dot.len() == 3 && after_dot.chars().all(|c| c.is_ascii_digit()) {
                match numeric_part.parse::<f64>() {
                    Ok(v) => Ok(ParsedNumber {
                        raw: raw.to_string(),
                        value: v,
                        ambiguous: true, // Could be EU thousands 1.234 = 1234
                        parse_strategy: "decimal_dot".to_string(),
                    }),
                    Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
                }
            } else {
                // Standard decimal dot
                match numeric_part.parse::<f64>() {
                    Ok(v) => Ok(ParsedNumber {
                        raw: raw.to_string(),
                        value: v,
                        ambiguous: false,
                        parse_strategy: "decimal_dot".to_string(),
                    }),
                    Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
                }
            }
        } else {
            // Multiple dots: assume thousands separators (1.234.567) - EU format
            let normalized = numeric_part.replace('.', "");
            match normalized.parse::<f64>() {
                Ok(v) => Ok(ParsedNumber {
                    raw: raw.to_string(),
                    value: v,
                    ambiguous: false,
                    parse_strategy: "thousands_dot".to_string(),
                }),
                Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
            }
        }
    }
    // Case 4: No separators - plain integer
    else {
        match numeric_part.parse::<f64>() {
            Ok(v) => Ok(ParsedNumber {
                raw: raw.to_string(),
                value: v,
                ambiguous: false,
                parse_strategy: "plain".to_string(),
            }),
            Err(_) => Err(format!("Failed to parse '{}' as number", raw)),
        }
    }
}

/// Parse a number with biomarker context hint for disambiguation
///
/// When the biomarker is known, we can use expected value ranges to disambiguate:
/// - Glucose "5,8" in EU context is 5.8 mmol/L (not 58)
/// - Cholesterol "1,234" is likely 1.234 mmol/L (not 1234)
///
/// # Arguments
/// * `raw` - The raw string to parse
/// * `typical_max` - Maximum typical value for this biomarker (if known)
///
/// # Returns
/// * `Ok(ParsedNumber)` - Successfully parsed number, potentially re-interpreted
/// * `Err(String)` - Parse failure with reason
pub fn parse_number_with_hint(raw: &str, typical_max: Option<f64>) -> Result<ParsedNumber, String> {
    let mut result = parse_number(raw)?;

    // If ambiguous and we have a typical max, try to disambiguate
    if result.ambiguous {
        if let Some(max) = typical_max {
            // Check if interpreting as thousands separator makes more sense
            if result.parse_strategy == "thousands_comma" && result.value > max * 10.0 {
                // Value seems way too high - try decimal comma interpretation
                let trimmed = raw.trim();
                let numeric_part = trimmed
                    .trim_start_matches("<=")
                    .trim_start_matches(">=")
                    .trim_start_matches('<')
                    .trim_start_matches('>')
                    .trim();

                let normalized = numeric_part.replace(',', ".");
                if let Ok(v) = normalized.parse::<f64>() {
                    if v <= max * 2.0 {
                        result.value = v;
                        result.parse_strategy = "decimal_comma".to_string();
                        result.ambiguous = false;
                    }
                }
            }
            // Similarly for decimal_dot that might be EU thousands
            else if result.parse_strategy == "decimal_dot" && result.value < max / 100.0 {
                // Value seems way too low - might be EU thousands separator
                let trimmed = raw.trim();
                let numeric_part = trimmed
                    .trim_start_matches("<=")
                    .trim_start_matches(">=")
                    .trim_start_matches('<')
                    .trim_start_matches('>')
                    .trim();

                let normalized = numeric_part.replace('.', "");
                if let Ok(v) = normalized.parse::<f64>() {
                    if v >= max / 10.0 && v <= max * 2.0 {
                        result.value = v;
                        result.parse_strategy = "thousands_dot".to_string();
                        result.ambiguous = false;
                    }
                }
            }
        }
    }

    Ok(result)
}

// ============================================================================
// Comparator and biomarker types
// ============================================================================

/// Value comparator for lab results (e.g., "<5" means less than 5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum Comparator {
    #[default]
    #[serde(rename = "=")]
    Eq, // = (exact value)
    #[serde(rename = "<")]
    Lt, // <
    #[serde(rename = ">")]
    Gt, // >
    #[serde(rename = "<=")]
    Le, // <=
    #[serde(rename = ">=")]
    Ge, // >=
}

impl Comparator {
    /// Parse a comparator string into a Comparator enum
    pub fn from_str(s: &str) -> Comparator {
        match s.trim() {
            "<" => Comparator::Lt,
            ">" => Comparator::Gt,
            "<=" | "≤" => Comparator::Le,
            ">=" | "≥" => Comparator::Ge,
            _ => Comparator::Eq,
        }
    }

    /// Returns true if this is the default Eq comparator
    pub fn is_eq(&self) -> bool {
        *self == Comparator::Eq
    }
}

/// A parsed biomarker extracted from input
#[derive(Debug, Clone, serde::Serialize)]
pub struct ParsedBiomarker {
    /// The original name as found in input
    pub name: String,
    /// Standardized identifier (e.g. "hba1c")
    pub standardized_name: String,
    /// Human-friendly display name
    pub display_name: String,
    /// Parsed numeric value
    pub value: f64,
    /// Unit as found in input (or standard unit)
    pub unit: String,
    /// Category inferred from definitions
    pub category: String,
    /// Whether the marker was successfully resolved
    pub resolved: bool,
    /// Confidence level: exact, normalized, inferred_from_unit, ambiguous
    pub confidence: String,
    /// How the resolution was achieved
    pub resolution_method: String,
    /// Value comparator (<, >, <=, >=) - defaults to Eq (exact value)
    #[serde(skip_serializing_if = "Comparator::is_eq")]
    pub comparator: Comparator,
}

/// Attempt to normalize a raw biomarker name using the structured catalog.
/// Returns (standardized_name, display_name, category, confidence, resolution_method)
pub fn normalize_name(
    raw: &str,
    value: Option<f64>,
    unit: Option<&str>,
) -> Option<(String, String, String, String, String)> {
    let resolved = catalog::resolve(raw, value, unit)?;
    Some((
        resolved.canonical_id,
        resolved.display_name,
        resolved.category,
        resolved.confidence.as_str().to_string(),
        resolved.resolution_method.as_str().to_string(),
    ))
}

/// Normalize a unit string to its canonical form.
///
/// This is the critical unit equivalence layer — all downstream tools (labassess, labstore)
/// rely on labparse outputting canonical unit strings. Every variant of a unit must map to
/// exactly one canonical form so that range matching and trend calculations work correctly.
///
/// Design: Many-to-one mapping. Variations like "IU/L", "iu/l", "U/L", "u/l" all map to
/// a single canonical string. The canonical form uses standard SI/clinical conventions.
pub fn normalize_unit(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();

    // Strip common prefixes/suffixes that don't change the unit semantics
    let cleaned = lower
        .replace("×", "x")
        .replace("*", "x")
        .replace("^", "");

    match cleaned.as_str() {
        // === MASS CONCENTRATIONS ===
        "mg/dl" | "mg/dl." => "mg/dL".into(),
        "ng/ml" | "ng/ml." => "ng/mL".into(),
        "pg/ml" | "pg/ml." => "pg/mL".into(),
        "ug/dl" | "µg/dl" | "mcg/dl" => "µg/dL".into(),
        // µg/L and ng/mL are equivalent (1 µg/L = 1 ng/mL) — canonicalize to ng/mL
        "ug/l" | "µg/l" | "mcg/l" => "ng/mL".into(),
        // ng/L = pg/mL (equivalent: 1 ng/L = 1 pg/mL) — canonicalize to pg/mL
        "ng/l" => "pg/mL".into(),
        "mg/l" => "mg/L".into(),
        "g/l" | "gm/l" => "g/L".into(),
        "g/dl" | "gm/dl" => "g/dL".into(),

        // === MOLAR CONCENTRATIONS ===
        "mmol/l" | "mmol/l." => "mmol/L".into(),
        "umol/l" | "µmol/l" | "mcmol/l" => "µmol/L".into(),
        "nmol/l" | "nmol/l." => "nmol/L".into(),
        "pmol/l" | "pmol/l." => "pmol/L".into(),

        // === HbA1c UNITS ===
        "mmol/mol" => "mmol/mol".into(),
        // % handled below

        // === ENZYME ACTIVITY — IU/L and U/L are equivalent ===
        "iu/l" | "u/l" | "iu/l." => "U/L".into(),
        "iu/ml" | "u/ml" => "U/mL".into(),
        "ku/l" | "kiu/l" => "kU/L".into(),
        "miu/l" | "uiu/l" | "µiu/l" => "mIU/L".into(),
        "miu/ml" | "uiu/ml" | "µiu/ml" => "mIU/mL".into(), // Keep /mL separate from /L

        // === CELL COUNTS ===
        // All variants of x10^9/L → canonical "x10^9/L"
        "x109/l" | "10^9/l" | "109/l" | "x10e9/l"
        | "x 109/l" | "x10^9/l" | "10^9/l." | "thou/ul" | "k/ul"
        | "x109/l." => "x10^9/L".into(),
        // Bare /µL stays as-is — don't assume cell count context
        "/ul" | "/µl" => "/µL".into(),
        // All variants of x10^12/L → canonical "x10^12/L"
        "x1012/l" | "10^12/l" | "1012/l" | "x10e12/l"
        | "x 1012/l" | "x10^12/l" | "m/ul" | "mil/ul" => "x10^12/L".into(),

        // === RENAL ===
        "ml/min/1.73m2" | "ml/min/1.73m²" | "ml/min/1.73m2." => "mL/min/1.73m²".into(),
        "ml/min" => "mL/min".into(), // Don't add BSA normalization

        // === HAEMATOLOGY ===
        "fl" | "fl." => "fL".into(),
        "pg" | "pg." => "pg".into(),
        "l/l" => "L/L".into(),
        "mm/hr" | "mm/h" | "mm/hr." => "mm/hr".into(),

        // === ELECTROLYTES ===
        "meq/l" => "mEq/L".into(),

        // === COAGULATION ===
        "seconds" | "sec" | "secs" => "s".into(),

        // === PERCENTAGES ===
        "%" | "percent" => "%".into(),

        // === OTHER ===
        "ratio" => "ratio".into(),
        "mcg" | "ug" | "µg" => "µg".into(),
        "mg" => "mg".into(),
        "ctrl unit" => "ctrl unit".into(),
        "titer" => "titer".into(),
        "/hpf" => "/HPF".into(),
        "cells/ul" | "cells/µl" => "cells/µL".into(),
        "ph" => "pH".into(),

        // Fallback: return trimmed original (preserve case for unknown units)
        _ => trimmed.to_string(),
    }
}

// ============================================================================
// Unit tests for locale-aware number parsing
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // === Basic parsing tests ===

    #[test]
    fn test_plain_integer() {
        let result = parse_number("42").unwrap();
        assert_eq!(result.value, 42.0);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "plain");
    }

    #[test]
    fn test_plain_negative() {
        let result = parse_number("-17").unwrap();
        assert_eq!(result.value, -17.0);
        assert!(!result.ambiguous);
    }

    #[test]
    fn test_decimal_dot_simple() {
        let result = parse_number("5.8").unwrap();
        assert_eq!(result.value, 5.8);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_dot");
    }

    #[test]
    fn test_decimal_dot_with_leading_zero() {
        let result = parse_number("0.25").unwrap();
        assert_eq!(result.value, 0.25);
        assert!(!result.ambiguous);
    }

    // === EU decimal comma tests ===

    #[test]
    fn test_decimal_comma_single_digit() {
        let result = parse_number("5,8").unwrap();
        assert_eq!(result.value, 5.8);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_comma");
    }

    #[test]
    fn test_decimal_comma_two_digits() {
        let result = parse_number("1,25").unwrap();
        assert_eq!(result.value, 1.25);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_comma");
    }

    // === Thousands separator tests ===

    #[test]
    fn test_thousands_comma_us() {
        let result = parse_number("1,234").unwrap();
        assert_eq!(result.value, 1234.0);
        assert!(result.ambiguous); // Could be EU decimal 1,234 = 1.234
        assert_eq!(result.parse_strategy, "thousands_comma");
    }

    #[test]
    fn test_thousands_comma_large() {
        let result = parse_number("1,234,567").unwrap();
        assert_eq!(result.value, 1234567.0);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "thousands_comma");
    }

    #[test]
    fn test_thousands_dot_eu_multiple() {
        let result = parse_number("1.234.567").unwrap();
        assert_eq!(result.value, 1234567.0);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "thousands_dot");
    }

    // === Mixed separator tests ===

    #[test]
    fn test_us_format_thousands_and_decimal() {
        let result = parse_number("1,234.56").unwrap();
        assert_eq!(result.value, 1234.56);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_dot");
    }

    #[test]
    fn test_eu_format_thousands_and_decimal() {
        let result = parse_number("1.234,56").unwrap();
        assert_eq!(result.value, 1234.56);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_comma");
    }

    // === Comparator handling tests ===

    #[test]
    fn test_less_than_comparator() {
        let result = parse_number("<5").unwrap();
        assert_eq!(result.value, 5.0);
    }

    #[test]
    fn test_greater_than_comparator() {
        let result = parse_number(">100").unwrap();
        assert_eq!(result.value, 100.0);
    }

    #[test]
    fn test_less_than_or_equal_comparator() {
        let result = parse_number("<=0.5").unwrap();
        assert_eq!(result.value, 0.5);
    }

    #[test]
    fn test_greater_than_or_equal_comparator() {
        let result = parse_number(">=10,5").unwrap();
        assert_eq!(result.value, 10.5);
    }

    #[test]
    fn test_unicode_comparator_le() {
        let result = parse_number("≤5").unwrap();
        assert_eq!(result.value, 5.0);
    }

    #[test]
    fn test_unicode_comparator_ge() {
        let result = parse_number("≥10").unwrap();
        assert_eq!(result.value, 10.0);
    }

    // === Whitespace handling tests ===

    #[test]
    fn test_leading_whitespace() {
        let result = parse_number("  5.8").unwrap();
        assert_eq!(result.value, 5.8);
    }

    #[test]
    fn test_trailing_whitespace() {
        let result = parse_number("5.8  ").unwrap();
        assert_eq!(result.value, 5.8);
    }

    #[test]
    fn test_whitespace_around_comparator() {
        let result = parse_number("< 5").unwrap();
        assert_eq!(result.value, 5.0);
    }

    // === Edge cases ===

    #[test]
    fn test_empty_string_error() {
        let result = parse_number("");
        assert!(result.is_err());
    }

    #[test]
    fn test_whitespace_only_error() {
        let result = parse_number("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_comparator_only_error() {
        let result = parse_number("<");
        assert!(result.is_err());
    }

    #[test]
    fn test_ambiguous_dot_three_digits() {
        // 1.234 could be decimal (1.234) or EU thousands (1234)
        let result = parse_number("1.234").unwrap();
        assert_eq!(result.value, 1.234); // Prefer decimal interpretation
        assert!(result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_dot");
    }

    // === Hint-based disambiguation tests ===

    #[test]
    fn test_hint_disambiguates_thousands_comma() {
        // "1,234" as thousands = 1234, but if max is 10, that's way too high
        // Should re-interpret as decimal comma = 1.234
        let result = parse_number_with_hint("1,234", Some(10.0)).unwrap();
        assert_eq!(result.value, 1.234);
        assert!(!result.ambiguous);
        assert_eq!(result.parse_strategy, "decimal_comma");
    }

    #[test]
    fn test_hint_keeps_valid_thousands() {
        // "1,234" with max 10000 -> 1234 is reasonable
        let result = parse_number_with_hint("1,234", Some(10000.0)).unwrap();
        assert_eq!(result.value, 1234.0);
        assert!(result.ambiguous); // Still ambiguous but not reinterpreted
    }

    #[test]
    fn test_hint_none_no_change() {
        let result = parse_number_with_hint("1,234", None).unwrap();
        assert_eq!(result.value, 1234.0);
        assert!(result.ambiguous);
    }

    // === Real-world biomarker value tests ===

    #[test]
    fn test_glucose_eu_format() {
        // EU glucose: 5,8 mmol/L
        let result = parse_number("5,8").unwrap();
        assert_eq!(result.value, 5.8);
    }

    #[test]
    fn test_cholesterol_eu_format() {
        // EU cholesterol: 4,5 mmol/L
        let result = parse_number("4,5").unwrap();
        assert_eq!(result.value, 4.5);
    }

    #[test]
    fn test_hba1c_eu_format() {
        // EU HbA1c: 5,8%
        let result = parse_number("5,8").unwrap();
        assert_eq!(result.value, 5.8);
    }

    #[test]
    fn test_large_cell_count() {
        // WBC count: 7,500 /µL (US format)
        let result = parse_number("7,500").unwrap();
        assert_eq!(result.value, 7500.0);
    }

    #[test]
    fn test_preserves_raw_string() {
        let result = parse_number("  1,234.56  ").unwrap();
        assert_eq!(result.raw, "  1,234.56  ");
    }
}
