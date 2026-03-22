pub mod json;
pub mod table;

/// Check if stdout is a terminal (TTY)
pub fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}
