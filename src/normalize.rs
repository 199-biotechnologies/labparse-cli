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

/// Normalize a unit string, cleaning up common variations
pub fn normalize_unit(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.to_lowercase().as_str() {
        "mg/dl" => "mg/dL".to_string(),
        "ng/ml" => "ng/mL".to_string(),
        "pg/ml" => "pg/mL".to_string(),
        "ug/dl" | "µg/dl" | "mcg/dl" => "µg/dL".to_string(),
        "ug/l" | "µg/l" | "mcg/l" => "µg/L".to_string(),
        "ng/l" => "ng/L".to_string(),
        "iu/l" => "IU/L".to_string(),
        "iu/ml" => "IU/mL".to_string(),
        "ku/l" => "kU/L".to_string(),
        "u/l" => "U/L".to_string(),
        "u/ml" => "U/mL".to_string(),
        "miu/l" => "mIU/L".to_string(),
        "g/l" => "g/L".to_string(),
        "g/dl" => "g/dL".to_string(),
        "mmol/l" => "mmol/L".to_string(),
        "umol/l" | "µmol/l" | "mcmol/l" => "µmol/L".to_string(),
        "nmol/l" => "nmol/L".to_string(),
        "pmol/l" => "pmol/L".to_string(),
        "ml/min/1.73m2" | "ml/min/1.73m²" => "mL/min/1.73m²".to_string(),
        "mm/hr" | "mm/h" => "mm/hr".to_string(),
        "%" | "percent" => "%".to_string(),
        "ratio" => "ratio".to_string(),
        "mcg" | "ug" | "µg" => "µg".to_string(),
        "mg" => "mg".to_string(),
        _ => trimmed.to_string(),
    }
}
