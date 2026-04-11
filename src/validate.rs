//! Validation stack for parsed lab results.
//!
//! Runs after extraction to catch:
//! - Physiological plausibility violations (impossible values)
//! - Cross-marker arithmetic inconsistencies (CBC differential sum, lipid math)
//! - Reference range vs flag consistency (value outside range but not flagged)
//!
//! Validators may:
//! - Add warnings to the result
//! - Promote document_status to NeedsReview or Failed
//! - Set flagged=true on individual biomarkers

use crate::normalize::ParsedBiomarker;
use crate::parsers::{DocumentStatus, ParseResult};

/// Run all validators against a ParseResult, mutating it in place.
pub fn validate(result: &mut ParseResult) {
    let mut downgrade_to = result.document_status;

    // Run sub-validators
    let v1 = check_plausibility(&mut result.biomarkers);
    let v2 = check_cbc_differential_sum(&result.biomarkers);
    let v3 = check_friedewald(&result.biomarkers);
    let v4 = check_tc_hdl_ratio(&result.biomarkers);
    let v5 = check_reference_range_consistency(&mut result.biomarkers);

    for v in [v1, v2, v3, v4, v5] {
        result.warnings.extend(v.warnings);
        downgrade_to = worse_status(downgrade_to, v.suggested_status);
    }

    result.document_status = downgrade_to;
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub warnings: Vec<String>,
    pub suggested_status: DocumentStatus,
}

impl ValidationResult {
    fn new() -> Self {
        Self {
            warnings: Vec::new(),
            suggested_status: DocumentStatus::Complete,
        }
    }
    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
        if matches!(self.suggested_status, DocumentStatus::Complete) {
            self.suggested_status = DocumentStatus::NeedsReview;
        }
    }
    fn fail(&mut self, msg: String) {
        self.warnings.push(msg);
        self.suggested_status = DocumentStatus::PartialFailure;
    }
}

fn worse_status(a: DocumentStatus, b: DocumentStatus) -> DocumentStatus {
    use DocumentStatus::*;
    let rank = |s: DocumentStatus| match s {
        Complete => 0,
        NeedsReview => 1,
        PartialFailure => 2,
    };
    if rank(a) >= rank(b) { a } else { b }
}

/// V1: Physiological plausibility bands.
/// Hardcoded soft bounds for common biomarkers — flag values outside as suspicious.
/// Hard bounds (impossible) → Failed status.
/// Soft bounds (unusual) → NeedsReview.
fn check_plausibility(biomarkers: &mut [ParsedBiomarker]) -> ValidationResult {
    let mut result = ValidationResult::new();

    for bm in biomarkers.iter_mut() {
        if let Some((hard_lo, hard_hi, soft_lo, soft_hi)) = plausibility_band(&bm.standardized_name, &bm.unit) {
            if bm.value < hard_lo || bm.value > hard_hi {
                result.fail(format!(
                    "IMPOSSIBLE value: {} = {} {} (hard bounds: {}-{})",
                    bm.display_name, bm.value, bm.unit, hard_lo, hard_hi
                ));
                bm.flagged = true;
            } else if bm.value < soft_lo || bm.value > soft_hi {
                result.warn(format!(
                    "Unusual value: {} = {} {} (typical: {}-{})",
                    bm.display_name, bm.value, bm.unit, soft_lo, soft_hi
                ));
                bm.flagged = true;
            }
        }
    }

    result
}

/// Returns (hard_min, hard_max, soft_min, soft_max) for a biomarker if known.
/// Hard = physiologically impossible. Soft = unusual but possible.
fn plausibility_band(std_name: &str, unit: &str) -> Option<(f64, f64, f64, f64)> {
    match (std_name, unit) {
        // Glucose
        ("fasting_glucose", "mg/dL") => Some((20.0, 800.0, 50.0, 250.0)),
        ("fasting_glucose", "mmol/L") => Some((1.1, 44.4, 2.8, 13.9)),
        ("glucose", "mg/dL") => Some((20.0, 800.0, 50.0, 300.0)),
        ("glucose", "mmol/L") => Some((1.1, 44.4, 2.8, 16.7)),
        // HbA1c
        ("hba1c", "%") => Some((2.0, 20.0, 3.5, 14.0)),
        ("hba1c", "mmol/mol") => Some((4.0, 195.0, 15.0, 130.0)),
        // Lipids
        ("total_cholesterol", "mg/dL") => Some((30.0, 700.0, 80.0, 400.0)),
        ("total_cholesterol", "mmol/L") => Some((0.78, 18.1, 2.0, 10.3)),
        ("ldl_cholesterol", "mg/dL") => Some((10.0, 600.0, 30.0, 350.0)),
        ("ldl_cholesterol", "mmol/L") => Some((0.26, 15.5, 0.78, 9.0)),
        ("hdl_cholesterol", "mg/dL") => Some((5.0, 200.0, 15.0, 120.0)),
        ("hdl_cholesterol", "mmol/L") => Some((0.13, 5.2, 0.39, 3.1)),
        ("triglycerides", "mg/dL") => Some((10.0, 5000.0, 30.0, 1000.0)),
        ("triglycerides", "mmol/L") => Some((0.11, 56.5, 0.34, 11.3)),
        // Liver
        ("alt", "U/L") => Some((1.0, 5000.0, 5.0, 500.0)),
        ("ast", "U/L") => Some((1.0, 5000.0, 5.0, 500.0)),
        ("alp", "U/L") => Some((10.0, 2000.0, 30.0, 500.0)),
        ("albumin", "g/dL") => Some((1.0, 7.0, 2.5, 5.5)),
        ("albumin", "g/L") => Some((10.0, 70.0, 25.0, 55.0)),
        ("total_bilirubin", "mg/dL") => Some((0.05, 50.0, 0.1, 5.0)),
        ("total_bilirubin", "umol/L") => Some((0.85, 855.0, 1.7, 85.0)),
        // Hematology
        ("hemoglobin", "g/dL") => Some((3.0, 25.0, 8.0, 20.0)),
        ("hemoglobin", "g/L") => Some((30.0, 250.0, 80.0, 200.0)),
        ("hematocrit", "%") => Some((10.0, 75.0, 25.0, 60.0)),
        ("wbc_count", "x10^9/L") => Some((0.1, 200.0, 1.0, 50.0)),
        ("platelet_count", "x10^9/L") => Some((5.0, 2000.0, 30.0, 800.0)),
        // Kidney
        ("creatinine", "mg/dL") => Some((0.1, 30.0, 0.3, 10.0)),
        ("creatinine", "µmol/L") => Some((9.0, 2650.0, 27.0, 884.0)),
        // Thyroid
        ("tsh", "mIU/L") => Some((0.001, 500.0, 0.1, 100.0)),
        // Differential percentages must be 0-100
        ("neutrophils_pct", "%") => Some((0.0, 100.0, 5.0, 95.0)),
        ("lymphocytes_pct", "%") => Some((0.0, 100.0, 5.0, 80.0)),
        ("monocytes_pct", "%") => Some((0.0, 100.0, 0.0, 30.0)),
        ("eosinophils_pct", "%") => Some((0.0, 100.0, 0.0, 30.0)),
        ("basophils_pct", "%") => Some((0.0, 100.0, 0.0, 10.0)),
        _ => None,
    }
}

/// V2: CBC differential percentages should sum to ~100%.
fn check_cbc_differential_sum(biomarkers: &[ParsedBiomarker]) -> ValidationResult {
    let mut result = ValidationResult::new();

    let pct_markers = ["neutrophils_pct", "lymphocytes_pct", "monocytes_pct", "eosinophils_pct", "basophils_pct"];
    let mut sum = 0.0;
    let mut count = 0;

    for std_name in &pct_markers {
        if let Some(bm) = biomarkers.iter().find(|b| b.standardized_name == *std_name && b.unit == "%") {
            sum += bm.value;
            count += 1;
        }
    }

    // Need at least 3 to make the check meaningful
    if count >= 3 {
        let deviation = (sum - 100.0).abs();
        if deviation > 5.0 {
            result.warn(format!(
                "CBC differential sum is {:.1}% (expected ~100%, found {} markers)",
                sum, count
            ));
        }
    }

    result
}

/// V3: Friedewald equation: TC ≈ LDL + HDL + TG/5 (mg/dL) or TG/2.2 (mmol/L)
fn check_friedewald(biomarkers: &[ParsedBiomarker]) -> ValidationResult {
    let mut result = ValidationResult::new();

    let tc = biomarkers.iter().find(|b| b.standardized_name == "total_cholesterol");
    let ldl = biomarkers.iter().find(|b| b.standardized_name == "ldl_cholesterol");
    let hdl = biomarkers.iter().find(|b| b.standardized_name == "hdl_cholesterol");
    let tg = biomarkers.iter().find(|b| b.standardized_name == "triglycerides");

    if let (Some(tc), Some(ldl), Some(hdl), Some(tg)) = (tc, ldl, hdl, tg) {
        // Make sure all are in same unit family
        if tc.unit != ldl.unit || tc.unit != hdl.unit || tc.unit != tg.unit {
            return result; // Mixed units, skip
        }

        let divisor = match tc.unit.as_str() {
            "mg/dL" => 5.0,
            "mmol/L" => 2.2,
            _ => return result,
        };

        let computed_tc = ldl.value + hdl.value + tg.value / divisor;
        let deviation_pct = ((computed_tc - tc.value).abs() / tc.value) * 100.0;

        if deviation_pct > 15.0 {
            result.warn(format!(
                "Friedewald check failed: TC={} but LDL+HDL+TG/{}={:.2} ({:.1}% deviation)",
                tc.value, divisor, computed_tc, deviation_pct
            ));
        }
    }

    result
}

/// V4: TC/HDL ratio should match TC/HDL division
fn check_tc_hdl_ratio(biomarkers: &[ParsedBiomarker]) -> ValidationResult {
    let mut result = ValidationResult::new();

    let ratio = biomarkers.iter().find(|b| b.standardized_name == "tc_hdl_ratio");
    let tc = biomarkers.iter().find(|b| b.standardized_name == "total_cholesterol");
    let hdl = biomarkers.iter().find(|b| b.standardized_name == "hdl_cholesterol");

    if let (Some(ratio), Some(tc), Some(hdl)) = (ratio, tc, hdl) {
        if hdl.value > 0.0 && tc.unit == hdl.unit {
            let computed = tc.value / hdl.value;
            let deviation_pct = ((computed - ratio.value).abs() / ratio.value) * 100.0;
            if deviation_pct > 10.0 {
                result.warn(format!(
                    "TC/HDL ratio mismatch: reported {} but TC/HDL = {:.2} ({:.1}% deviation)",
                    ratio.value, computed, deviation_pct
                ));
            }
        }
    }

    result
}

/// V5: Reference range consistency.
/// If biomarker has a reference_range and the value is outside it but not flagged → warn.
fn check_reference_range_consistency(biomarkers: &mut [ParsedBiomarker]) -> ValidationResult {
    let mut result = ValidationResult::new();

    for bm in biomarkers.iter_mut() {
        let Some(ref_range) = bm.reference_range.clone() else { continue };

        if let Some((lo, hi)) = parse_reference_range(&ref_range) {
            let outside = bm.value < lo || bm.value > hi;
            if outside && !bm.flagged {
                bm.flagged = true;
                result.warn(format!(
                    "{} = {} {} is outside reference range {} but was not flagged",
                    bm.display_name, bm.value, bm.unit, ref_range
                ));
            }
        }
    }

    result
}

/// Parse a reference range string into (low, high) bounds.
/// Handles: "4.3 - 5.4", "4.3-5.4", "<5.0", "> 1.0", "(2.6 - 24.9)"
fn parse_reference_range(s: &str) -> Option<(f64, f64)> {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')').trim();

    // "<X" or "<= X"
    if let Some(rest) = s.strip_prefix("<=").or_else(|| s.strip_prefix('<')) {
        let v: f64 = rest.trim().parse().ok()?;
        return Some((f64::NEG_INFINITY, v));
    }
    // ">X" or ">= X"
    if let Some(rest) = s.strip_prefix(">=").or_else(|| s.strip_prefix('>')) {
        let v: f64 = rest.trim().parse().ok()?;
        return Some((v, f64::INFINITY));
    }

    // "X - Y" or "X-Y"
    if let Some(idx) = s.find(" - ") {
        let lo: f64 = s[..idx].trim().parse().ok()?;
        let hi: f64 = s[idx + 3..].trim().parse().ok()?;
        return Some((lo, hi));
    }
    if let Some(idx) = s.find('-') {
        if idx > 0 {
            let lo_str = s[..idx].trim();
            let hi_str = s[idx + 1..].trim();
            let lo: f64 = lo_str.parse().ok()?;
            let hi: f64 = hi_str.parse().ok()?;
            return Some((lo, hi));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reference_range() {
        assert_eq!(parse_reference_range("4.3 - 5.4"), Some((4.3, 5.4)));
        assert_eq!(parse_reference_range("4.3-5.4"), Some((4.3, 5.4)));
        assert_eq!(parse_reference_range("<5.0"), Some((f64::NEG_INFINITY, 5.0)));
        assert_eq!(parse_reference_range("> 1.0"), Some((1.0, f64::INFINITY)));
        assert_eq!(parse_reference_range("(2.6 - 24.9)"), Some((2.6, 24.9)));
    }

    #[test]
    fn test_plausibility_glucose_impossible() {
        use crate::normalize::{Comparator, UnitStatus};
        let mut bms = vec![ParsedBiomarker {
            name: "Glucose".into(),
            standardized_name: "glucose".into(),
            display_name: "Glucose".into(),
            value: 5000.0,
            unit: "mg/dL".into(),
            category: "metabolic".into(),
            resolved: true,
            confidence: "exact".into(),
            resolution_method: "exact_alias".into(),
            comparator: Comparator::Eq,
            reference_range: None,
            flagged: false,
            unit_status: UnitStatus::Observed,
            page: None,
            raw_value_text: None,
            raw_unit: None,
            source_text: None,
        }];
        let result = check_plausibility(&mut bms);
        assert!(!result.warnings.is_empty());
        assert!(matches!(result.suggested_status, DocumentStatus::PartialFailure));
        assert!(bms[0].flagged);
    }
}
