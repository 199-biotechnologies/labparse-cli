use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

const BIOMARKER_CSV: &str = include_str!("../data/biomarker_definitions.csv");

#[derive(Debug, Clone, Serialize)]
pub struct BiomarkerDef {
    pub marker_name: String,
    pub standardized_name: String,
    pub display_name: String,
    pub abbreviation: String,
    pub standard_unit: String,
    pub standard_range_min: Option<f64>,
    pub standard_range_max: Option<f64>,
    pub aggressive_target_min: Option<f64>,
    pub aggressive_target_max: Option<f64>,
    pub category: String,
}

/// All biomarker definitions keyed by standardized_name
pub static DEFINITIONS: Lazy<HashMap<String, BiomarkerDef>> = Lazy::new(load_definitions);

/// Lookup map: lowercase alias → standardized_name
pub static ALIAS_MAP: Lazy<BTreeMap<String, String>> = Lazy::new(build_alias_map);

fn parse_opt_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

fn infer_category(name: &str, unit: &str) -> String {
    let n = name.to_lowercase();
    let u = unit.to_lowercase();

    static CATEGORY_MAPPING: &[(&[&str], &str)] = &[
        (&["steps", "calories", "distance", "strain", "sleep", "hrv", "heart_rate", "spo2",
           "respiratory", "readiness", "recovery", "awake", "energy_expenditure", "activity",
           "body_temperature", "resting_heart_rate"], "wearable"),
        (&["cholesterol", "ldl", "hdl", "triglyceride", "apob", "apoa", "lipoprotein",
           "small_dense"], "lipid"),
        (&["glucose", "hba1c", "insulin", "c_peptide", "fasting", "adiponectin", "leptin",
           "resistin"], "metabolic"),
        (&["tsh", "t3", "t4", "thyro", "trab", "ft3", "ft4", "reverse_t3"], "thyroid"),
        (&["testosterone", "estradiol", "dhea", "shbg", "prolactin", "cortisol", "acth",
           "pth", "igf", "fai", "beta_hcg"], "hormone"),
        (&["crp", "hscrp", "il_6", "tnf", "esr", "ferritin", "fibrinogen", "homocysteine"], "inflammation"),
        (&["alt", "ast", "ggt", "alp", "bilirubin", "albumin"], "liver"),
        (&["creatinine", "urea", "egfr", "cystatin", "uric_acid", "bun"], "kidney"),
        (&["rbc", "wbc", "platelet", "hemoglobin", "hematocrit", "mcv", "mch", "rdw", "mpv",
           "neutrophil", "lymphocyte", "monocyte", "eosinophil", "nlr"], "hematology"),
        (&["vitamin", "zinc", "magnesium", "iron", "calcium", "copper", "potassium", "sodium",
           "phosphate", "tibc", "transferrin", "folic", "b12"], "nutritional"),
        (&["iga", "igg", "igm", "complement", "anti_ccp", "anti_tpo", "anti_tg", "rf", "aso"], "immunology"),
        (&["psa", "cea", "ca_125", "ca_15", "ca_19", "afp"], "cancer_marker"),
        (&["tau", "amyloid", "nfl", "gfap", "alpha_synuclein", "bd_tau"], "neurological"),
        (&["galectin", "tmao", "tas"], "cardiovascular"),
    ];

    for (keywords, category) in CATEGORY_MAPPING {
        if keywords.iter().any(|k| n.contains(k)) {
            return category.to_string();
        }
    }

    if n.starts_with("urine") {
        return "urinalysis".to_string();
    }

    if u.contains("score") || u.contains("ratio") || u.contains("profile") {
        return "composite".to_string();
    }

    "other".to_string()
}

fn load_definitions() -> HashMap<String, BiomarkerDef> {
    let mut map = HashMap::new();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(BIOMARKER_CSV.as_bytes());

    for result in rdr.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => continue,
        };
        let get = |i: usize| record.get(i).unwrap_or("").to_string();

        let standardized_name = get(1);
        let standard_unit = get(4);

        let def = BiomarkerDef {
            marker_name: get(0),
            standardized_name: standardized_name.clone(),
            display_name: get(2),
            abbreviation: get(3),
            category: infer_category(&standardized_name, &standard_unit),
            standard_unit,
            standard_range_min: parse_opt_f64(&get(5)),
            standard_range_max: parse_opt_f64(&get(6)),
            aggressive_target_min: parse_opt_f64(&get(7)),
            aggressive_target_max: parse_opt_f64(&get(8)),
        };
        map.entry(standardized_name).or_insert(def);
    }
    map
}

fn build_alias_map() -> BTreeMap<String, String> {
    let mut aliases: BTreeMap<String, String> = BTreeMap::new();

    // Sort keys for deterministic iteration order
    let mut sorted_keys: Vec<&String> = DEFINITIONS.keys().collect();
    sorted_keys.sort();

    for std_name in sorted_keys {
        let def = &DEFINITIONS[std_name];
        // Add all the direct names
        let candidates = [
            &def.marker_name,
            &def.standardized_name,
            &def.display_name,
            &def.abbreviation,
        ];
        for name in candidates {
            let lower = name.to_lowercase().trim().to_string();
            if !lower.is_empty() {
                aliases.entry(lower).or_insert_with(|| std_name.clone());
            }
        }

        // Underscore → space variants
        let with_spaces = std_name.replace('_', " ");
        aliases
            .entry(with_spaces.to_lowercase())
            .or_insert_with(|| std_name.clone());

        // Dash variants
        let with_dashes = std_name.replace('_', "-");
        aliases
            .entry(with_dashes.to_lowercase())
            .or_insert_with(|| std_name.clone());
    }

    // ── Extra hand-crafted aliases for common lab-report names ──
    let extras: &[(&str, &str)] = &[
        // HbA1c
        ("hemoglobin a1c", "hba1c"),
        ("hba1c", "hba1c"),
        ("hb a1c", "hba1c"),
        ("a1c", "hba1c"),
        ("glycated hemoglobin", "hba1c"),
        ("glycosylated hemoglobin", "hba1c"),
        // LDL
        ("ldl", "ldl_cholesterol"),
        ("ldl-c", "ldl_cholesterol"),
        ("ldl cholesterol", "ldl_cholesterol"),
        ("low density lipoprotein", "ldl_cholesterol"),
        // HDL
        ("hdl", "hdl_cholesterol"),
        ("hdl-c", "hdl_cholesterol"),
        ("hdl cholesterol", "hdl_cholesterol"),
        ("high density lipoprotein", "hdl_cholesterol"),
        // Total cholesterol
        ("total cholesterol", "total_cholesterol"),
        ("cholesterol", "total_cholesterol"),
        ("tc", "total_cholesterol"),
        // Triglycerides
        ("trig", "triglycerides"),
        ("trigs", "triglycerides"),
        ("tg", "triglycerides"),
        // ApoB
        ("apob", "apolipoprotein_b"),
        ("apo b", "apolipoprotein_b"),
        ("apolipoprotein b", "apolipoprotein_b"),
        // ApoA1
        ("apoa1", "apolipoprotein_a1"),
        ("apo a1", "apolipoprotein_a1"),
        ("apoa-i", "apolipoprotein_a1"),
        ("apolipoprotein a1", "apolipoprotein_a1"),
        ("apolipoprotein a-i", "apolipoprotein_a1"),
        // Fasting glucose
        ("fasting glucose", "fasting_glucose"),
        ("glucose", "fasting_glucose"),
        ("blood glucose", "fasting_glucose"),
        ("blood sugar", "fasting_glucose"),
        ("fg", "fasting_glucose"),
        // hsCRP
        ("crp", "hscrp"),
        ("c-reactive protein", "hscrp"),
        ("hs-crp", "hscrp"),
        ("high sensitivity crp", "hscrp"),
        ("c reactive protein", "hscrp"),
        // Liver
        ("alanine aminotransferase", "alt"),
        ("sgpt", "alt"),
        ("aspartate aminotransferase", "ast"),
        ("sgot", "ast"),
        ("gamma-glutamyltransferase", "ggt"),
        ("gamma gt", "ggt"),
        ("alkaline phosphatase", "alp"),
        // Kidney
        ("bun", "urea"),
        ("blood urea nitrogen", "urea"),
        ("estimated gfr", "egfr"),
        ("glomerular filtration rate", "egfr"),
        // Thyroid
        ("thyroid stimulating hormone", "tsh"),
        ("free t3", "free_t3"),
        ("free t4", "free_t4"),
        // Iron
        ("total iron binding capacity", "tibc"),
        ("tibc", "tibc"),
        ("tsat", "transferrin_saturation"),
        // Vitamins
        ("vitamin d", "vitamin_d"),
        ("25-oh vitamin d", "vitamin_d"),
        ("25(oh)d", "vitamin_d"),
        ("25-hydroxyvitamin d", "vitamin_d"),
        ("vitamin b12", "vitamin_b12"),
        ("b12", "vitamin_b12"),
        ("folate", "folic_acid"),
        // Hematology
        ("rbc", "rbc_count"),
        ("red blood cells", "rbc_count"),
        ("wbc", "wbc_count"),
        ("white blood cells", "wbc_count"),
        ("plt", "platelets"),
        ("platelet count", "platelets"),
        ("hgb", "hemoglobin"),
        ("hb", "hemoglobin"),
        ("hct", "hematocrit"),
        // Hormones
        ("dhea-s", "dhea_s"),
        ("dhea sulfate", "dhea_s"),
        ("igf-1", "igf_1"),
        ("lp(a)", "lipoprotein_a"),
        ("lpa", "lipoprotein_a"),
        // PSA
        ("psa", "total_psa"),
        ("prostate specific antigen", "total_psa"),
        // Uric acid
        ("uric acid", "uric_acid"),
        ("ua", "uric_acid"),
        // Inflammation
        ("sed rate", "esr"),
        ("sedimentation rate", "esr"),
        ("interleukin 6", "il_6"),
        ("il-6", "il_6"),
        ("tnf-alpha", "tnf_alpha"),
        ("tnf alpha", "tnf_alpha"),
        ("homocysteine", "homocysteine"),
        ("hcy", "homocysteine"),
        // Thyroglobulin antibodies
        ("thyroglobulin antibodies", "anti_tg"),
        ("thyroglobulin antibodies (tg abs)", "anti_tg"),
        ("tg antibodies", "anti_tg"),
        ("tg abs", "anti_tg"),
        ("anti-thyroglobulin", "anti_tg"),
        ("atg", "anti_tg"),
    ];

    // Extras OVERRIDE CSV-derived aliases — these are hand-curated corrections
    // for cases where the CSV abbreviation creates a wrong mapping (e.g., "wbc"
    // from urine_wbc's abbreviation should really map to wbc_count)
    for (alias, std) in extras {
        aliases.insert(alias.to_lowercase(), std.to_string());
    }

    aliases
}

/// Try to resolve a name to a standardized biomarker name
pub fn resolve_name(input: &str) -> Option<&'static str> {
    let key = input.to_lowercase().trim().to_string();
    ALIAS_MAP.get(&key).and_then(|std_name| {
        // Return a &'static str from the DEFINITIONS map key
        DEFINITIONS.get_key_value(std_name).map(|(k, _)| k.as_str())
    })
}

/// Get the definition for a standardized name
pub fn get_definition(standardized_name: &str) -> Option<&'static BiomarkerDef> {
    DEFINITIONS.get(standardized_name)
}

/// Get all unique categories
pub fn categories() -> Vec<String> {
    let mut cats: Vec<String> = DEFINITIONS
        .values()
        .map(|d| d.category.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    cats.sort();
    cats
}

/// List all definitions, optionally filtered by category
pub fn list_all(category: Option<&str>) -> Vec<&'static BiomarkerDef> {
    let mut defs: Vec<_> = DEFINITIONS
        .values()
        .filter(|d| category.map_or(true, |c| d.category == c))
        .collect();
    defs.sort_by(|a, b| a.standardized_name.cmp(&b.standardized_name));
    defs
}
