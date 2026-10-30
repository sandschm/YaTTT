use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Nicht angemeldet")]
    NotAuthenticated,
    #[error("Sitzung abgelaufen oder keine Berechtigung (HTTP {0})")]
    AuthExpired(u16),
    #[error("Jira antwortete mit HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Netzwerkfehler: {0}")]
    Network(String),
    #[error("Ungültige URL: {0}")]
    InvalidUrl(String),
    #[error("Datenbankfehler: {0}")]
    Database(String),
    #[error("Schlüsselbund: {0}")]
    Keyring(String),
    #[error("Ein-/Ausgabefehler: {0}")]
    Io(String),
    #[error("Ungültige Daten: {0}")]
    Parse(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Nicht gefunden: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Database(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_decode() {
            Error::Parse(e.to_string())
        } else {
            Error::Network(e.to_string())
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error::InvalidUrl(e.to_string())
    }
}

impl From<keyring::Error> for Error {
    fn from(e: keyring::Error) -> Self {
        Error::Keyring(e.to_string())
    }
}

impl From<csv::Error> for Error {
    fn from(e: csv::Error) -> Self {
        Error::Io(e.to_string())
    }
}
