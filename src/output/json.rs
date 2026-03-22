use serde::Serialize;

use crate::normalize::ParsedBiomarker;
use crate::parsers::ParseResult;

#[derive(Serialize)]
pub struct JsonEnvelope {
    pub version: &'static str,
    pub status: &'static str,
    pub data: JsonData,
    pub metadata: JsonMetadata,
}

#[derive(Serialize)]
pub struct JsonData {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_detected: Option<String>,
    pub biomarkers: Vec<JsonBiomarker>,
    pub parse_warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct JsonBiomarker {
    pub name: String,
    pub standardized_name: String,
    pub display_name: String,
    pub value: f64,
    pub unit: String,
    pub category: String,
}

#[derive(Serialize)]
pub struct JsonMetadata {
    pub elapsed_ms: u128,
    pub markers_found: usize,
    pub parser: String,
}

impl From<&ParsedBiomarker> for JsonBiomarker {
    fn from(bm: &ParsedBiomarker) -> Self {
        Self {
            name: bm.name.clone(),
            standardized_name: bm.standardized_name.clone(),
            display_name: bm.display_name.clone(),
            value: bm.value,
            unit: bm.unit.clone(),
            category: bm.category.clone(),
        }
    }
}

pub fn render(result: &ParseResult, source: &str, elapsed_ms: u128) -> String {
    let envelope = JsonEnvelope {
        version: "1",
        status: "success",
        data: JsonData {
            source: source.to_string(),
            date_detected: None,
            biomarkers: result.biomarkers.iter().map(JsonBiomarker::from).collect(),
            parse_warnings: result.warnings.clone(),
        },
        metadata: JsonMetadata {
            elapsed_ms,
            markers_found: result.biomarkers.len(),
            parser: result.parser_name.clone(),
        },
    };
    serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| "{}".to_string())
}

pub fn render_error(err: &str) -> String {
    serde_json::json!({
        "version": "1",
        "status": "error",
        "error": err
    })
    .to_string()
}
