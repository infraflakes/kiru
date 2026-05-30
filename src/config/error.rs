use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    Io(#[from] std::io::Error),
    ParseReports(Vec<miette::Report>),
    CircularImport(String),
    Validation(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {}", e),
            ConfigError::ParseReports(reports) => {
                for report in reports {
                    writeln!(f, "{}", report)?;
                }
                Ok(())
            }
            ConfigError::CircularImport(path) => {
                write!(f, "Circular import detected: {}", path)
            }
            ConfigError::Validation(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}
