//! Error types used across the crate.

use thiserror::Error;

/// Canonical error type for all fluxo subsystems.
///
/// Errors are categorised so that [`FluxoError::is_transient`] can distinguish
/// temporary runtime failures (retried with backoff) from permanent ones.
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum FluxoError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Module error ({module}): {message}")]
    Module {
        module: &'static str,
        message: String,
    },

    #[error("Daemon IPC error: {0}")]
    Ipc(String),

    #[error("External system error: {0}")]
    System(String),

    #[error("Bluetooth error: {0}")]
    Bluetooth(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Hardware error: {0}")]
    Hardware(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Module disabled: {0}")]
    Disabled(String),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl FluxoError {
    /// Returns `true` for errors that represent likely-transient failures
    /// (IO, external systems, hardware) and should trigger exponential backoff
    /// rather than permanent cooldown.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Io(_)
                | Self::System(_)
                | Self::Bluetooth(_)
                | Self::Network(_)
                | Self::Hardware(_)
                | Self::Module { .. }
        )
    }
}

/// Crate-wide `Result` alias using [`FluxoError`].
pub type Result<T> = std::result::Result<T, FluxoError>;
