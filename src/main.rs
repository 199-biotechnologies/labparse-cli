mod catalog;
mod cli;
mod errors;
mod normalize;
mod output;
mod parsers;

use std::io::Read;
use std::time::Instant;

use clap::Parser;

use cli::{Cli, Commands};
use errors::LabParseError;

fn main() {
    let cli = Cli::parse();

    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::AgentInfo => {
                print_agent_info();
                return;
            }
            Commands::Biomarkers { category } => {
                print_biomarkers(category.as_deref(), cli.json);
                return;
            }
        }
    }

    let use_json = cli.json || !output::is_tty();

    match run(&cli) {
        Ok((result, source, elapsed)) => {
            if result.biomarkers.is_empty() {
                let err = LabParseError::NoBiomarkersFound;
                if use_json {
                    eprintln!("{}", output::json::render_error(&err.to_string()));
                } else {
                    eprintln!("error: {}", err);
                }
                err.exit();
            }
            if use_json {
                println!("{}", output::json::render(&result, &source, elapsed));
            } else {
                output::table::render(&result, &source, elapsed);
            }
        }
        Err(e) => {
            if use_json {
                eprintln!("{}", output::json::render_error(&e.to_string()));
            } else {
                eprintln!("error: {}", e);
            }
            e.exit();
        }
    }
}

fn run(cli: &Cli) -> Result<(parsers::ParseResult, String, u128), LabParseError> {
    let start = Instant::now();

    // Check if input is a PDF — route to vision pipeline
    if let Some(path) = &cli.input {
        if !path.exists() {
            return Err(LabParseError::FileNotFound(path.display().to_string()));
        }
        if is_pdf(path) {
            let result = parsers::pdf_parser::parse(path, cli.dpi, "")?;
            let elapsed = start.elapsed().as_millis();
            let source = format!("pdf:{}", path.display());
            return Ok((result, source, elapsed));
        }
    }

    // Non-PDF path: text/CSV/stdin
    let (content, source) = get_input(cli)?;
    let result = parsers::auto_parse(&content, &source)?;
    let elapsed = start.elapsed().as_millis();

    Ok((result, source, elapsed))
}

/// Detect PDF by extension or magic bytes
fn is_pdf(path: &std::path::Path) -> bool {
    if let Some(ext) = path.extension() {
        return ext.to_ascii_lowercase() == "pdf";
    }
    // Check magic bytes (%PDF-)
    if let Ok(bytes) = std::fs::read(path) {
        return bytes.starts_with(b"%PDF-");
    }
    false
}

fn get_input(cli: &Cli) -> Result<(String, String), LabParseError> {
    // Priority: --text > --stdin > file argument
    if let Some(text) = &cli.text {
        return Ok((text.clone(), "text-input".to_string()));
    }

    if cli.stdin || !output::is_tty() && cli.input.is_none() && cli.text.is_none() {
        // Check if there's actually data on stdin
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            return Err(LabParseError::NoInput);
        }
        return Ok((buf, "stdin".to_string()));
    }

    if let Some(path) = &cli.input {
        if !path.exists() {
            return Err(LabParseError::FileNotFound(
                path.display().to_string(),
            ));
        }
        let content = std::fs::read_to_string(path)?;
        return Ok((content, path.display().to_string()));
    }

    Err(LabParseError::NoInput)
}

fn print_agent_info() {
    let info = serde_json::json!({
        "name": "labparse",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Parse lab results (PDF, CSV, text) into structured biomarker JSON",
        "capabilities": [
            "pdf_vision_extraction",
            "csv_parsing",
            "text_parsing",
            "stdin_input",
            "biomarker_normalization",
            "structured_catalog",
            "disambiguation_table",
            "unit_compatibility_filter",
            "confidence_scoring",
            "dual_model_verification",
        ],
        "input_formats": ["pdf", "csv", "text", "stdin"],
        "output_formats": ["json", "table"],
        "biomarker_count": catalog::marker_count(),
        "alias_count": catalog::alias_count(),
        "categories": catalog::categories(),
        "usage": {
            "pdf": "labparse bloodwork.pdf",
            "file": "labparse bloodwork.csv",
            "text": "labparse --text 'HbA1c 5.8%, ApoB 95 mg/dL'",
            "stdin": "cat notes.txt | labparse --stdin",
            "json": "labparse results.csv --json",
            "verify": "labparse report.pdf --verify gemini"
        }
    });
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}

fn print_biomarkers(category: Option<&str>, json: bool) {
    let defs = catalog::list_all(category);

    if json {
        let items: Vec<_> = defs
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.id,
                    "display_name": d.display_name,
                    "component": d.component,
                    "specimen": d.specimen,
                    "allowed_units": d.allowed_units,
                    "category": d.category,
                    "loinc": d.loinc,
                    "alias_count": d.aliases.len(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
        return;
    }

    use comfy_table::{presets, Cell, ContentArrangement, Table};
    let mut table = Table::new();
    table
        .load_preset(presets::UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Name", "ID", "Units", "Category"]);

    for d in &defs {
        let units = if d.allowed_units.is_empty() {
            "-".to_string()
        } else {
            d.allowed_units.join(", ")
        };
        table.add_row(vec![
            Cell::new(&d.display_name),
            Cell::new(&d.id),
            Cell::new(&units),
            Cell::new(&d.category),
        ]);
    }

    println!("{table}");
    println!("\n  {} markers ({} aliases)", defs.len(), catalog::alias_count());
    if category.is_none() {
        println!("  Categories: {}", catalog::categories().join(", "));
    }
}
