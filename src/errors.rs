use std::process;

#[derive(thiserror::Error, Debug)]
pub enum LabParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("No input provided. Pass a file, --text, or --stdin")]
    NoInput,

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Parse failure: {0}")]
    ParseFailure(String),

    #[error("No biomarkers found in input")]
    NoBiomarkersFound,
}

impl LabParseError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NoInput | Self::FileNotFound(_) => 2,
            Self::ParseFailure(_) | Self::NoBiomarkersFound => 3,
            _ => 1,
        }
    }

    pub fn exit(&self) -> ! {
        process::exit(self.exit_code());
    }
}
