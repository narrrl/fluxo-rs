use thiserror::Error;

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

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, FluxoError>;
