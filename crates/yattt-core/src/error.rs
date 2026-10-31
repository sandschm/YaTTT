use crate::tr;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    NotAuthenticated,
    /// HTTP status that indicated an expired session or missing permission.
    AuthExpired(u16),
    Http {
        status: u16,
        message: String,
    },
    Network(String),
    InvalidUrl(String),
    Database(String),
    Keyring(String),
    Io(String),
    Parse(String),
    /// A complete, user-facing message.
    Invalid(String),
    NotFound(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotAuthenticated => f.write_str(tr!("Nicht angemeldet", "Not signed in")),
            Error::AuthExpired(code) => f.write_str(&tr!(
                "Sitzung abgelaufen oder keine Berechtigung (HTTP {code})",
                "Session expired or no permission (HTTP {code})",
                code = code
            )),
            Error::Http { status, message } => f.write_str(&tr!(
                "Jira antwortete mit HTTP {status}: {message}",
                "Jira answered with HTTP {status}: {message}",
                status = status,
                message = message
            )),
            Error::Network(e) => {
                f.write_str(&tr!("Netzwerkfehler: {e}", "Network error: {e}", e = e))
            }
            Error::InvalidUrl(e) => {
                f.write_str(&tr!("Ungültige URL: {e}", "Invalid URL: {e}", e = e))
            }
            Error::Database(e) => {
                f.write_str(&tr!("Datenbankfehler: {e}", "Database error: {e}", e = e))
            }
            Error::Keyring(e) => f.write_str(&tr!("Schlüsselbund: {e}", "Keychain: {e}", e = e)),
            Error::Io(e) => f.write_str(&tr!("Ein-/Ausgabefehler: {e}", "I/O error: {e}", e = e)),
            Error::Parse(e) => {
                f.write_str(&tr!("Ungültige Daten: {e}", "Invalid data: {e}", e = e))
            }
            Error::Invalid(e) => f.write_str(e),
            Error::NotFound(e) => f.write_str(&tr!("Nicht gefunden: {e}", "Not found: {e}", e = e)),
        }
    }
}

impl std::error::Error for Error {}

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
