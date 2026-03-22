mod biomarkers;
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

    let (content, source) = get_input(cli)?;
    let result = parsers::auto_parse(&content, &source)?;
    let elapsed = start.elapsed().as_millis();

    Ok((result, source, elapsed))
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
        "description": "Parse lab results (CSV, free text) into structured biomarker JSON",
        "capabilities": [
            "csv_parsing",
            "text_parsing",
            "stdin_input",
            "biomarker_normalization",
            "148_biomarker_definitions"
        ],
        "input_formats": ["csv", "text", "stdin"],
        "output_formats": ["json", "table"],
        "biomarker_count": biomarkers::DEFINITIONS.len(),
        "categories": biomarkers::categories(),
        "usage": {
            "file": "labparse bloodwork.csv",
            "text": "labparse --text 'HbA1c 5.8%, ApoB 95 mg/dL'",
            "stdin": "cat notes.txt | labparse --stdin",
            "json": "labparse results.csv --json"
        }
    });
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}

fn print_biomarkers(category: Option<&str>, json: bool) {
    let defs = biomarkers::list_all(category);

    if json {
        let items: Vec<_> = defs
            .iter()
            .map(|d| {
                serde_json::json!({
                    "standardized_name": d.standardized_name,
                    "display_name": d.display_name,
                    "abbreviation": d.abbreviation,
                    "unit": d.standard_unit,
                    "category": d.category,
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
        .set_header(vec!["Name", "Standard Name", "Unit", "Category"]);

    for d in &defs {
        table.add_row(vec![
            Cell::new(&d.display_name),
            Cell::new(&d.standardized_name),
            Cell::new(&d.standard_unit),
            Cell::new(&d.category),
        ]);
    }

    println!("{table}");
    println!("\n  {} biomarkers", defs.len());
    if category.is_none() {
        println!("  Categories: {}", biomarkers::categories().join(", "));
    }
}
