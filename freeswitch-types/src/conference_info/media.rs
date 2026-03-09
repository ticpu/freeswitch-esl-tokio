use std::fmt;
use std::str::FromStr;

/// Media stream descriptor for a conference endpoint (RFC 4575 Section 5.7).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Media {
    /// Unique stream identifier within the endpoint (required XML attribute).
    #[cfg_attr(feature = "serde", serde(rename = "@id", default))]
    pub id: String,
    /// Human-readable label (e.g. "main audio").
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// SDP media type: `audio`, `video`, `text`, `application`.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "type", default, skip_serializing_if = "Option::is_none")
    )]
    pub media_type: Option<String>,
    /// SSRC label or other stream identifier.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub label: Option<String>,
    /// SSRC source identifier.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "src-id", default, skip_serializing_if = "Option::is_none")
    )]
    pub src_id: Option<String>,
    /// Directionality of the media stream.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub status: Option<MediaStatus>,
}

impl Media {
    /// Create a media descriptor with the given stream ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_text: None,
            media_type: None,
            label: None,
            src_id: None,
            status: None,
        }
    }

    /// Set the media type.
    pub fn with_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    /// Set the media status.
    pub fn with_status(mut self, status: MediaStatus) -> Self {
        self.status = Some(status);
        self
    }
}

/// Media stream directionality (RFC 4575 Section 5.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum MediaStatus {
    /// Receive-only.
    #[cfg_attr(feature = "serde", serde(rename = "recvonly"))]
    RecvOnly,
    /// Send-only.
    #[cfg_attr(feature = "serde", serde(rename = "sendonly"))]
    SendOnly,
    /// Bidirectional.
    #[cfg_attr(feature = "serde", serde(rename = "sendrecv"))]
    SendRecv,
    /// Inactive (paused).
    #[cfg_attr(feature = "serde", serde(rename = "inactive"))]
    Inactive,
}

impl fmt::Display for MediaStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RecvOnly => "recvonly",
            Self::SendOnly => "sendonly",
            Self::SendRecv => "sendrecv",
            Self::Inactive => "inactive",
        })
    }
}

/// Error returned when parsing an invalid [`MediaStatus`] string.
#[derive(Debug, Clone)]
pub struct ParseMediaStatusError(pub String);

impl fmt::Display for ParseMediaStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid media status: {:?}", self.0)
    }
}

impl std::error::Error for ParseMediaStatusError {}

impl FromStr for MediaStatus {
    type Err = ParseMediaStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "recvonly" => Ok(Self::RecvOnly),
            "sendonly" => Ok(Self::SendOnly),
            "sendrecv" => Ok(Self::SendRecv),
            "inactive" => Ok(Self::Inactive),
            other => Err(ParseMediaStatusError(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_status_round_trip() {
        for (s, expected) in [
            ("recvonly", MediaStatus::RecvOnly),
            ("sendonly", MediaStatus::SendOnly),
            ("sendrecv", MediaStatus::SendRecv),
            ("inactive", MediaStatus::Inactive),
        ] {
            let parsed: MediaStatus = s
                .parse()
                .unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn media_builder() {
        let m = Media::new("1")
            .with_type("audio")
            .with_status(MediaStatus::SendRecv);
        assert_eq!(m.id, "1");
        assert_eq!(
            m.media_type
                .as_deref(),
            Some("audio")
        );
        assert_eq!(m.status, Some(MediaStatus::SendRecv));
    }
}
