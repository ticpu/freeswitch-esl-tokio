use std::fmt;

/// Errors from parsing or serializing RFC 4575 conference-info XML.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ConferenceInfoError {
    /// XML parsing or serialization error.
    Xml(String),
}

impl fmt::Display for ConferenceInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(msg) => write!(f, "conference-info XML error: {msg}"),
        }
    }
}

impl std::error::Error for ConferenceInfoError {}
