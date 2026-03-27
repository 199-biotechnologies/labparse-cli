use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "labparse",
    version,
    about = "Parse lab results into structured biomarker JSON"
)]
pub struct Cli {
    /// Input file (CSV, text, or PDF)
    pub input: Option<PathBuf>,

    /// Parse free-form text directly
    #[arg(long)]
    pub text: Option<String>,

    /// Read from stdin
    #[arg(long)]
    pub stdin: bool,

    /// Force JSON output (default when piped)
    #[arg(long, global = true)]
    pub json: bool,

    /// Quiet mode — suppress warnings
    #[arg(long, global = true)]
    pub quiet: bool,

    /// PDF rendering DPI (default: 150, higher = slower but more accurate)
    #[arg(long, default_value = "150")]
    pub dpi: u32,

    /// Vision backend: rapid (Rapid-MLX) or ollama
    #[arg(long, default_value = "rapid")]
    pub backend: String,

    /// Cross-verify PDF extraction with a second model (gemini or codex)
    #[arg(long)]
    pub verify: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show agent-info for AI consumption
    AgentInfo,
    /// List all known biomarkers
    Biomarkers {
        /// Filter by category
        #[arg(long)]
        category: Option<String>,
    },
}
