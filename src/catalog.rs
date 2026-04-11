//! Structured biomarker catalog with multi-step normalization pipeline.
//!
//! Design (approved by Codex + Gemini + Claude review):
//! - Structured entries: canonical_id, component, specimen, qualifier, display_name, aliases, allowed_units, loinc
//! - NO Levenshtein fuzzy matching (TSH↔FSH, ALT↔AST, LDL↔HDL all match at distance ≤ 2)
//! - Multi-step normalization: lowercase → strip specimen prefix → strip method suffix → British→American → noise removal → exact lookup
//! - Disambiguation table for dangerous ambiguous names (CRP/hsCRP, testosterone, bilirubin, calcium, glucose)
//! - Unit compatibility as hard filter
//! - Confidence + resolution_method in output
//! - Passthrough with structure (unresolved markers emit raw_name + resolved:false)

use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;

// ── Embedded default catalog ──
const EMBEDDED_CATALOG: &str = include_str!("../data/biomarkers.toml");

// ── Catalog data model ──

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogFile {
    #[serde(default)]
    pub marker: Vec<MarkerEntry>,
    #[serde(default)]
    pub disambiguation: Vec<DisambiguationEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // qualifier and allowed_units used during loading + future validation
pub struct MarkerEntry {
    pub id: String,
    pub component: String,
    #[serde(default)]
    pub specimen: String,
    #[serde(default)]
    pub qualifier: String,
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub allowed_units: Vec<String>,
    #[serde(default)]
    pub loinc: String,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisambiguationEntry {
    pub ambiguous_name: String,
    pub candidates: Vec<DisambiguationCandidate>,
    #[serde(default)]
    pub default_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisambiguationCandidate {
    pub id: String,
    #[serde(default)]
    pub when_unit_in: Vec<String>,
    #[serde(default)]
    pub when_value_below: Option<f64>,
    #[serde(default)]
    pub when_value_above: Option<f64>,
}

// ── Resolution result ──

#[derive(Debug, Clone)]
#[allow(dead_code)] // allowed_units used in normalize.rs and future validators
pub struct ResolvedMarker {
    pub canonical_id: String,
    pub display_name: String,
    pub category: String,
    pub allowed_units: Vec<String>,
    pub confidence: Confidence,
    pub resolution_method: ResolutionMethod,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Confidence {
    Exact,
    Normalized,
    InferredFromUnit,
    Ambiguous,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Normalized => "normalized",
            Self::InferredFromUnit => "inferred_from_unit",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // Unresolved variant used by future validators
pub enum ResolutionMethod {
    ExactAlias,
    NormalizedPipeline,
    Disambiguation,
    Unresolved,
}

impl ResolutionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExactAlias => "exact_alias",
            Self::NormalizedPipeline => "normalized_pipeline",
            Self::Disambiguation => "disambiguation",
            Self::Unresolved => "unresolved",
        }
    }
}

// ── Loaded catalog state ──

pub struct LoadedCatalog {
    pub markers: HashMap<String, MarkerEntry>,
    /// lowercase alias → canonical_id
    pub alias_map: HashMap<String, String>,
    /// lowercase ambiguous name → disambiguation entry
    pub disambiguations: HashMap<String, DisambiguationEntry>,
}

pub static CATALOG: Lazy<LoadedCatalog> = Lazy::new(load_catalog);

fn load_catalog() -> LoadedCatalog {
    let catalog: CatalogFile = toml::from_str(EMBEDDED_CATALOG)
        .expect("embedded biomarkers.toml must parse");

    // Try loading overrides from ~/.labstore/biomarkers.toml
    let overrides = load_external_catalog();

    let mut markers = HashMap::new();
    let mut alias_map = HashMap::new();

    // Process embedded catalog first
    for entry in &catalog.marker {
        register_entry(entry, &mut markers, &mut alias_map);
    }

    // Apply overrides (external entries win)
    if let Some(ext) = &overrides {
        for entry in &ext.marker {
            register_entry(entry, &mut markers, &mut alias_map);
        }
    }

    // Build disambiguation map
    let mut disambiguations = HashMap::new();
    for d in &catalog.disambiguation {
        disambiguations.insert(d.ambiguous_name.to_lowercase(), d.clone());
    }
    if let Some(ext) = &overrides {
        for d in &ext.disambiguation {
            disambiguations.insert(d.ambiguous_name.to_lowercase(), d.clone());
        }
    }

    LoadedCatalog { markers, alias_map, disambiguations }
}

fn register_entry(
    entry: &MarkerEntry,
    markers: &mut HashMap<String, MarkerEntry>,
    alias_map: &mut HashMap<String, String>,
) {
    let id = entry.id.clone();
    markers.insert(id.clone(), entry.clone());

    // Register canonical id as alias
    alias_map.insert(id.to_lowercase(), id.clone());

    // Register component name
    if !entry.component.is_empty() {
        alias_map.insert(entry.component.to_lowercase(), id.clone());
    }

    // Register display name
    if !entry.display_name.is_empty() {
        alias_map.insert(entry.display_name.to_lowercase(), id.clone());
    }

    // Register all explicit aliases
    for alias in &entry.aliases {
        alias_map.insert(alias.to_lowercase(), id.clone());
    }

    // Auto-generate underscore/dash/space variants of the id
    let with_spaces = id.replace('_', " ");
    alias_map.entry(with_spaces.to_lowercase()).or_insert_with(|| id.clone());
    let with_dashes = id.replace('_', "-");
    alias_map.entry(with_dashes.to_lowercase()).or_insert_with(|| id.clone());
}

fn load_external_catalog() -> Option<CatalogFile> {
    // Check env var first (for testing)
    if let Ok(val) = std::env::var("LABPARSE_CATALOG") {
        if val == "none" {
            return None;
        }
        let content = std::fs::read_to_string(&val).ok()?;
        return toml::from_str(&content).ok();
    }

    let home = dirs::home_dir()?;
    let path = home.join(".labstore").join("biomarkers.toml");
    if path.exists() {
        let content = std::fs::read_to_string(&path).ok()?;
        toml::from_str(&content).ok()
    } else {
        None
    }
}

// ── Normalization pipeline ──

/// British → American spelling map
static BRITISH_AMERICAN: &[(&str, &str)] = &[
    ("haemoglobin", "hemoglobin"),
    ("haematocrit", "hematocrit"),
    ("haematology", "hematology"),
    ("oestradiol", "estradiol"),
    ("oestrogen", "estrogen"),
    ("oestriol", "estriol"),
    ("foetal", "fetal"),
    ("faecal", "fecal"),
    ("tumour", "tumor"),
    ("colour", "color"),
    ("behaviour", "behavior"),
    ("fibre", "fiber"),
    ("centre", "center"),
    ("litre", "liter"),
    ("metre", "meter"),
    ("sulphate", "sulfate"),
    ("aluminium", "aluminum"),
    ("anaemia", "anemia"),
    ("leukaemia", "leukemia"),
    ("leucocyte", "leukocyte"),
    ("leucocytes", "leukocytes"),
    ("diarrhoea", "diarrhea"),
    ("coeliac", "celiac"),
    ("paediatric", "pediatric"),
    ("programme", "program"),
    ("glycosylated", "glycated"),
];

/// Specimen prefixes to strip
static SPECIMEN_PREFIXES: &[&str] = &[
    "serum ", "plasma ", "whole blood ", "blood ", "urine ",
    "csf ", "cerebrospinal fluid ", "saliva ", "capillary ",
    "venous ", "arterial ",
];

/// Method suffixes to strip (with parentheses)
static METHOD_SUFFIXES: &[&str] = &[
    "(enzymatic)", "(jaffe)", "(ckd-epi)", "(ckd-epi 2021)", "(mdrd)",
    "(calculated)", "(direct)", "(indirect)", "(immunoassay)",
    "(hplc)", "(chemiluminescence)", "(elisa)", "(turbidimetric)",
    "(nephelometric)", "(colorimetric)", "(electrochemiluminescence)",
    "(ecl)", "(eia)", "(ria)", "(clia)", "(ifcc)", "(ngsp)",
    "(friedewald)", "(martin-hopkins)",
];

/// Noise words to remove
static NOISE_WORDS: &[&str] = &[
    "level", "levels", "test", "assay", "measurement", "determination",
    "analysis", "panel", "profile", "screen", "screening",
    "quantitative", "qualitative", "automated", "manual",
];

/// Italian → English translations for common biomarkers
static ITALIAN_TRANSLATIONS: &[(&str, &str)] = &[
    ("colesterolo totale", "total cholesterol"),
    ("colesterolo hdl", "hdl cholesterol"),
    ("colesterolo ldl", "ldl cholesterol"),
    ("trigliceridi", "triglycerides"),
    ("emoglobina", "hemoglobin"),
    ("emoglobina glicata", "hba1c"),
    ("ematocrito", "hematocrit"),
    ("eritrociti", "rbc count"),
    ("leucociti", "wbc count"),
    ("piastrine", "platelets"),
    ("glucosio", "fasting glucose"),
    ("glicemia", "fasting glucose"),
    ("creatinina", "creatinine"),
    ("acido urico", "uric acid"),
    ("bilirubina totale", "total bilirubin"),
    ("bilirubina diretta", "direct bilirubin"),
    ("transaminasi got", "ast"),
    ("transaminasi gpt", "alt"),
    ("fosfatasi alcalina", "alp"),
    ("gamma gt", "ggt"),
    ("azotemia", "urea"),
    ("sideremia", "iron"),
    ("ferritina", "ferritin"),
    ("vitamina d", "vitamin d"),
    ("vitamina b12", "vitamin b12"),
    ("acido folico", "folic acid"),
    ("proteina c reattiva", "hscrp"),
    ("velocita di eritrosedimentazione", "esr"),
    ("ves", "esr"),
    ("tireotropina", "tsh"),
    ("tiroxina libera", "free t4"),
    ("triiodotironina libera", "free t3"),
    ("testosterone totale", "testosterone"),
    ("antigene prostatico specifico", "total psa"),
    ("sodio", "sodium"),
    ("potassio", "potassium"),
    ("calcio", "calcium"),
    ("magnesio", "magnesium"),
    ("fosforo", "phosphate"),
    ("cloro", "chloride"),
    ("albumina", "albumin"),
    ("proteine totali", "total protein"),
    ("fibrinogeno", "fibrinogen"),
    ("omocisteina", "homocysteine"),
];

/// Apply the full normalization pipeline to a raw biomarker name.
/// Returns a normalized string suitable for exact alias lookup.
pub fn normalize_pipeline(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();

    // Step 0: Italian/foreign translations (check before other normalization)
    for (italian, english) in ITALIAN_TRANSLATIONS {
        if s == *italian {
            s = english.to_string();
            break;
        }
    }

    // Step 1: Strip specimen prefixes
    for prefix in SPECIMEN_PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }

    // Step 1b: Strip specimen suffixes after comma (e.g. "Calcium, Serum" → "calcium")
    // Only strip pure specimen markers — NOT qualifiers like ", Fasting" or ", Total"
    // which are clinically meaningful (catalog handles them via aliases).
    for suffix in &[
        ", serum", ", plasma", ", whole blood", ", blood", ", urine",
        ", random",
    ] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.trim().to_string();
            break;
        }
    }

    // Step 1c: Strip spaceless specimen suffixes (e.g. "Microalbumin Random" → "microalbumin")
    for suffix in &[" serum", " plasma", " whole blood"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.trim().to_string();
            break;
        }
    }

    // Step 2: Strip method suffixes
    for suffix in METHOD_SUFFIXES {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.trim().to_string();
            break;
        }
    }

    // Step 2b: Strip trailing parenthetical abbreviations like "(ALT)", "(TSH)", "(eGFR)"
    // Common in Randox UK reports: "Alanine Aminotransferase (ALT)"
    if let Some(paren_start) = s.rfind('(') {
        if s.ends_with(')') {
            let inside = &s[paren_start + 1..s.len() - 1];
            // Only strip if the parenthetical is short (abbreviation, not method)
            if inside.len() <= 10 && !inside.contains(' ') {
                s = s[..paren_start].trim().to_string();
            }
        }
    }

    // Step 3: British → American spelling
    for (british, american) in BRITISH_AMERICAN {
        if s.contains(british) {
            s = s.replace(british, american);
        }
    }

    // Step 4: CamelCase split (e.g., "RedBloodCells" → "red blood cells")
    // Only if string has no spaces already
    if !s.contains(' ') && s.chars().any(|c| c.is_uppercase()) {
        let original = raw.trim(); // use pre-lowercase for case detection
        let mut words = Vec::new();
        let mut current = String::new();
        for ch in original.chars() {
            if ch.is_uppercase() && !current.is_empty() {
                words.push(current.to_lowercase());
                current.clear();
            }
            current.push(ch);
        }
        if !current.is_empty() {
            words.push(current.to_lowercase());
        }
        if words.len() > 1 {
            let joined = words.join(" ");
            // Only use CamelCase result if it's different from just lowercasing
            if joined != s {
                s = joined;
            }
        }
    }

    // Step 5: Remove noise words
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() > 1 {
        let filtered: Vec<&str> = words
            .iter()
            .filter(|w| !NOISE_WORDS.contains(w))
            .copied()
            .collect();
        if !filtered.is_empty() && filtered.len() < words.len() {
            s = filtered.join(" ");
        }
    }

    // Step 6: Normalize common punctuation
    s = s.replace("–", "-").replace("—", "-");

    s.trim().to_string()
}

// ── Public resolver API ──

/// Resolve a raw biomarker name to a catalog entry.
/// Uses exact alias lookup after normalization pipeline.
/// Does NOT do fuzzy matching (by design — safety over coverage).
pub fn resolve(raw_name: &str, value: Option<f64>, unit: Option<&str>) -> Option<ResolvedMarker> {
    let cat = &*CATALOG;
    let lower = raw_name.trim().to_lowercase();

    // Step 1: Exact alias lookup (pre-normalization)
    if let Some(id) = cat.alias_map.get(&lower) {
        if let Some(entry) = cat.markers.get(id) {
            // Check if this is an ambiguous name
            if let Some(disamb) = cat.disambiguations.get(&lower) {
                return Some(resolve_disambiguation(disamb, entry, value, unit));
            }
            // Unit compatibility check
            if unit_compatible(entry, unit) {
                return Some(make_resolved(entry, Confidence::Exact, ResolutionMethod::ExactAlias));
            }
        }
    }

    // Step 2: Normalized pipeline lookup
    let normalized = normalize_pipeline(raw_name);
    if normalized != lower {
        if let Some(id) = cat.alias_map.get(&normalized) {
            if let Some(entry) = cat.markers.get(id) {
                if let Some(disamb) = cat.disambiguations.get(&normalized) {
                    return Some(resolve_disambiguation(disamb, entry, value, unit));
                }
                if unit_compatible(entry, unit) {
                    return Some(make_resolved(entry, Confidence::Normalized, ResolutionMethod::NormalizedPipeline));
                }
            }
        }
    }

    // Step 3: Check disambiguation table directly (for partial matches)
    for (amb_name, disamb) in &cat.disambiguations {
        if lower == *amb_name || normalized == *amb_name {
            if let Some(default_entry) = cat.markers.get(&disamb.default_id) {
                return Some(resolve_disambiguation(disamb, default_entry, value, unit));
            }
        }
    }

    None
}

fn resolve_disambiguation(
    disamb: &DisambiguationEntry,
    default_entry: &MarkerEntry,
    value: Option<f64>,
    unit: Option<&str>,
) -> ResolvedMarker {
    let unit_lower = unit.map(|u| u.to_lowercase());

    for cand in &disamb.candidates {
        let mut matches = true;

        // Check unit constraint
        if !cand.when_unit_in.is_empty() {
            if let Some(ref u) = unit_lower {
                if !cand.when_unit_in.iter().any(|allowed| allowed.to_lowercase() == *u) {
                    matches = false;
                }
            }
        }

        // Check value constraints
        if let (Some(v), Some(below)) = (value, cand.when_value_below) {
            if v >= below {
                matches = false;
            }
        }
        if let (Some(v), Some(above)) = (value, cand.when_value_above) {
            if v <= above {
                matches = false;
            }
        }

        if matches {
            if let Some(entry) = CATALOG.markers.get(&cand.id) {
                let confidence = if unit.is_some() || value.is_some() {
                    Confidence::InferredFromUnit
                } else {
                    Confidence::Ambiguous
                };
                return make_resolved(entry, confidence, ResolutionMethod::Disambiguation);
            }
        }
    }

    // No candidate matched — only fall back to default if we have NO evidence to disambiguate
    // If we had unit or value but still couldn't match, the evidence was insufficient → ambiguous
    if unit.is_none() && value.is_none() {
        // No evidence at all — use default with Ambiguous confidence
        return make_resolved(default_entry, Confidence::Ambiguous, ResolutionMethod::Disambiguation);
    }

    // Had evidence but no candidate matched — return default but mark as ambiguous
    // This ensures downstream can flag it for review
    make_resolved(default_entry, Confidence::Ambiguous, ResolutionMethod::Disambiguation)
}

fn unit_compatible(entry: &MarkerEntry, unit: Option<&str>) -> bool {
    // If no unit provided or no allowed_units specified, don't filter
    if entry.allowed_units.is_empty() {
        return true;
    }
    let unit = match unit {
        Some(u) if !u.is_empty() => u,
        _ => return true,
    };
    let u_lower = unit.to_lowercase();
    // Normalize both sides for comparison (the input unit may have been
    // normalized by normalize_unit but the catalog uses canonical forms)
    let u_norm = crate::normalize::normalize_unit(unit).to_lowercase();
    entry.allowed_units.iter().any(|allowed| {
        let a_lower = allowed.to_lowercase();
        let a_norm = crate::normalize::normalize_unit(allowed).to_lowercase();
        a_lower == u_lower || a_lower == u_norm || a_norm == u_lower || a_norm == u_norm
    })
}

fn make_resolved(entry: &MarkerEntry, confidence: Confidence, method: ResolutionMethod) -> ResolvedMarker {
    ResolvedMarker {
        canonical_id: entry.id.clone(),
        display_name: entry.display_name.clone(),
        category: entry.category.clone(),
        allowed_units: entry.allowed_units.clone(),
        confidence,
        resolution_method: method,
    }
}

/// Get a marker entry by canonical ID
pub fn get_marker(id: &str) -> Option<&'static MarkerEntry> {
    CATALOG.markers.get(id)
}

/// Get all unique categories
pub fn categories() -> Vec<String> {
    let mut cats: Vec<String> = CATALOG.markers.values()
        .map(|m| m.category.clone())
        .filter(|c| !c.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    cats.sort();
    cats
}

/// List all marker entries, optionally filtered by category
pub fn list_all(category: Option<&str>) -> Vec<&'static MarkerEntry> {
    let mut entries: Vec<_> = CATALOG.markers.values()
        .filter(|m| category.is_none_or(|c| m.category == c))
        .collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

/// Total number of markers in catalog
pub fn marker_count() -> usize {
    CATALOG.markers.len()
}

/// Total number of aliases
pub fn alias_count() -> usize {
    CATALOG.alias_map.len()
}
