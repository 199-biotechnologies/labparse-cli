use comfy_table::{presets, Cell, CellAlignment, ContentArrangement, Table};
use owo_colors::OwoColorize;

use crate::parsers::ParseResult;

pub fn render(result: &ParseResult, source: &str, elapsed_ms: u128) {
    if result.biomarkers.is_empty() {
        eprintln!("{}", "No biomarkers found.".yellow());
        return;
    }

    let unresolved_count = result.unresolved.len();
    println!(
        "\n {} {} biomarkers from {}{}",
        "✓".green().bold(),
        result.biomarkers.len(),
        source.bold(),
        if unresolved_count > 0 {
            format!(" ({} unresolved)\n", unresolved_count)
        } else {
            "\n".to_string()
        }
    );

    let mut table = Table::new();
    table
        .load_preset(presets::UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Biomarker").set_alignment(CellAlignment::Left),
            Cell::new("Value").set_alignment(CellAlignment::Right),
            Cell::new("Unit").set_alignment(CellAlignment::Left),
            Cell::new("Category").set_alignment(CellAlignment::Left),
        ]);

    for bm in &result.biomarkers {
        table.add_row(vec![
            Cell::new(&bm.display_name),
            Cell::new(format_value(bm.value)),
            Cell::new(&bm.unit),
            Cell::new(&bm.category),
        ]);
    }

    println!("{table}");

    if !result.warnings.is_empty() {
        println!();
        for w in &result.warnings {
            eprintln!("  {} {}", "⚠".yellow(), w);
        }
    }

    println!(
        "\n  {} parsed by {} in {}ms\n",
        "ℹ".blue(),
        result.parser_name,
        elapsed_ms
    );
}

fn format_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1_000_000.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v)
    }
}
