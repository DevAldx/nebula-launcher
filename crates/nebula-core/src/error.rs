use std::{io, path::PathBuf};

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("error de E/S en {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("JSON inválido en {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{0}")]
    Validation(String),
    #[error("la instancia {0} no existe")]
    NotFound(String),
    #[error("no se pudieron determinar los directorios del usuario")]
    NoHomeDir,
    #[error("error de red: {0}")]
    Network(String),
    #[error("verificación de integridad fallida: {0}")]
    Integrity(String),
    #[error("{0}")]
    Launch(String),
    #[error("{0}")]
    Auth(String),
    /// Credenciales rechazadas de forma definitiva (invalid_grant/invalid_token).
    /// Los errores de red o de servidor no deben borrar la sesión guardada.
    #[error("{0}")]
    AuthExpired(String),
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}
