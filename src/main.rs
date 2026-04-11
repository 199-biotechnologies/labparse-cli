mod catalog;
mod cli;
mod errors;
mod normalize;
mod output;
mod parsers;
mod validate;

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
        Ok((mut result, source, elapsed)) => {
            // Run validation stack (plausibility, cross-marker math, ref range consistency)
            validate::validate(&mut result);

            // Only error if BOTH resolved and unresolved are empty
            if result.biomarkers.is_empty() && result.unresolved.is_empty() {
                let err = LabParseError::NoBiomarkersFound;
                if use_json {
                    eprintln!("{}", output::json::render_error(&err.to_string()));
                } else {
                    eprintln!("error: {}", err);
                }
                err.exit();
            }

            // Warn based on document status
            match result.document_status {
                parsers::DocumentStatus::NeedsReview => {
                    eprintln!(
                        "warning: document needs review — {} resolved, {} unresolved, {} conflicts",
                        result.biomarkers.len(),
                        result.unresolved.len(),
                        result.conflicts.len()
                    );
                }
                parsers::DocumentStatus::PartialFailure => {
                    let failed_pages: Vec<_> = result.page_statuses.iter()
                        .filter(|p| p.status == parsers::PageExtractStatus::Failed)
                        .map(|p| p.page.to_string())
                        .collect();
                    eprintln!(
                        "warning: partial failure — pages {} failed",
                        failed_pages.join(", ")
                    );
                }
                parsers::DocumentStatus::Complete => {}
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

    // Check if input is a PDF or image — route to vision pipeline
    if let Some(path) = &cli.input {
        if !path.exists() {
            return Err(LabParseError::FileNotFound(path.display().to_string()));
        }
        if is_pdf(path) {
            // Try born-digital text extraction first (fast, reliable)
            if let Ok(Some(text)) = parsers::pdf_parser::extract_text_from_pdf(path) {
                // Verify single patient across all pages
                let pages = parsers::pdf_parser::split_into_pages(&text);
                if let Err(e) = parsers::pdf_parser::verify_single_patient(&pages) {
                    return Err(LabParseError::ParseFailure(format!(
                        "Multi-patient document detected — refusing to merge: {}", e
                    )));
                }

                // Step 1: Try regex-based parsing (instant, handles simple formats)
                let mut result = parsers::auto_parse(&text, "pdftotext")?;
                let enough_markers = result.biomarkers.len() >= 3
                    && result.unresolved.len() < result.biomarkers.len() * 5;
                if enough_markers {
                    result.parser_name = "pdf-text".to_string();
                    let elapsed = start.elapsed().as_millis();
                    let source = format!("pdf:{}", path.display());
                    return Ok((result, source, elapsed));
                }

                // Step 2: Regex insufficient — use LLM to structure the text
                eprintln!(
                    "info: regex found {} markers ({} unresolved), using LLM",
                    result.biomarkers.len(), result.unresolved.len()
                );
                match parsers::pdf_parser::llm_structure_text_paged(&text) {
                    Ok(page_results) if !page_results.is_empty() => {
                        // Verify each page's markers against the page text
                        let pages = parsers::pdf_parser::split_into_pages(&text);
                        let mut verified_pages = Vec::new();
                        let mut total_original = 0;
                        let mut total_rejected = 0;
                        let mut all_verify_warnings = Vec::new();

                        for pr in page_results {
                            let page_text = pages.get(pr.page.saturating_sub(1))
                                .cloned()
                                .unwrap_or_default();
                            let original_count = pr.markers.len();
                            total_original += original_count;
                            let mut verify_warnings = Vec::new();
                            let verified = parsers::pdf_parser::verify_against_source(
                                pr.markers, &page_text, &mut verify_warnings,
                            );
                            total_rejected += original_count - verified.len();
                            all_verify_warnings.extend(verify_warnings);
                            verified_pages.push(parsers::pdf_parser::make_page_result_with_error(
                                pr.page, verified, pr.error,
                            ));
                        }

                        for w in &all_verify_warnings {
                            eprintln!("info: {}", w);
                        }

                        let mut result = parsers::pdf_parser::resolve_results(verified_pages)?;
                        result.warnings.extend(all_verify_warnings);
                        result.parser_name = "pdf-llm".to_string();

                        if total_rejected > 0 {
                            result.warnings.push(format!(
                                "Lexical verification rejected {} of {} markers — possible hallucination",
                                total_rejected, total_original
                            ));
                            if result.document_status == parsers::DocumentStatus::Complete {
                                result.document_status = parsers::DocumentStatus::NeedsReview;
                            }
                        }

                        let elapsed = start.elapsed().as_millis();
                        let source = format!("pdf:{}", path.display());
                        return Ok((result, source, elapsed));
                    }
                    Ok(_) => eprintln!("info: LLM returned 0 markers, trying vision model"),
                    Err(e) => eprintln!("info: LLM failed ({}), trying vision model", e),
                }
            }
            // Fall back to VLM for scanned PDFs or when all else fails
            let result = parsers::pdf_parser::parse(path, cli.dpi, "")?;
            let elapsed = start.elapsed().as_millis();
            let source = format!("pdf:{}", path.display());
            return Ok((result, source, elapsed));
        }
        if is_image(path) {
            let result = parsers::pdf_parser::parse_image(path)?;
            let elapsed = start.elapsed().as_millis();
            let source = format!("image:{}", path.display());
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
        return ext.eq_ignore_ascii_case("pdf");
    }
    if let Ok(bytes) = std::fs::read(path) {
        return bytes.starts_with(b"%PDF-");
    }
    false
}

/// Detect image by extension
fn is_image(path: &std::path::Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_ascii_lowercase();
        return matches!(ext.to_str(), Some("jpg" | "jpeg" | "png" | "webp" | "heic" | "tiff" | "bmp"));
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
            "json": "labparse results.csv --json"
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
