use crate::biomarkers;

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
}

/// Attempt to normalize a raw biomarker name, returning (standardized_name, display_name, category)
pub fn normalize_name(raw: &str) -> Option<(&'static str, String, String)> {
    let std_name = biomarkers::resolve_name(raw)?;
    let def = biomarkers::get_definition(std_name)?;
    Some((std_name, def.display_name.clone(), def.category.clone()))
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
        "ug/l" | "µg/l" | "mcg/l" | "ng/ml" => "ng/mL".into(),
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
        "miu/l" | "miu/ml" | "uiu/ml" | "µiu/ml" => "mIU/L".into(),

        // === CELL COUNTS ===
        // All variants of x10^9/L → canonical "x10^9/L"
        "x109/l" | "10^9/l" | "109/l" | "x10e9/l" | "/ul"
        | "x 109/l" | "x10^9/l" | "10^9/l." | "thou/ul" | "k/ul"
        | "x109/l." => "x10^9/L".into(),
        // All variants of x10^12/L → canonical "x10^12/L"
        "x1012/l" | "10^12/l" | "1012/l" | "x10e12/l"
        | "x 1012/l" | "x10^12/l" | "m/ul" | "mil/ul" => "x10^12/L".into(),

        // === RENAL ===
        "ml/min/1.73m2" | "ml/min/1.73m²" | "ml/min/1.73m2." | "ml/min" => "mL/min/1.73m²".into(),

        // === HAEMATOLOGY ===
        "fl" | "fl." => "fL".into(),
        "pg" | "pg." => "pg".into(),
        "l/l" => "L/L".into(),
        "mm/hr" | "mm/h" | "mm/hr." => "mm/hr".into(),

        // === PERCENTAGES ===
        "%" | "percent" => "%".into(),

        // === OTHER ===
        "ratio" => "ratio".into(),
        "mcg" | "ug" | "µg" => "µg".into(),
        "mg" => "mg".into(),
        "ctrl unit" => "ctrl unit".into(),

        // Fallback: return trimmed original (preserve case for unknown units)
        _ => trimmed.to_string(),
    }
}
