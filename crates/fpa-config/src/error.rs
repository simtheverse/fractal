use std::fmt;

/// Errors that can occur during composition fragment operations.
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// Failed to parse TOML input.
    ParseError(String),
    /// Circular extends chain detected.
    CircularExtends(String),
    /// Referenced fragment name not found in registry.
    UnknownFragment(String),
    /// Fragment failed validation.
    ValidationError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::ParseError(msg) => write!(f, "parse error: {}", msg),
            ConfigError::CircularExtends(msg) => write!(f, "circular extends: {}", msg),
            ConfigError::UnknownFragment(msg) => write!(f, "unknown fragment: {}", msg),
            ConfigError::ValidationError(msg) => write!(f, "validation error: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}
