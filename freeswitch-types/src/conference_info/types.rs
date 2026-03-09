use super::common::State;
use super::media::MediaStatus;

/// Conference metadata (RFC 4575 Section 5.1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct ConferenceDescription {
    /// Conference display name.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// Conference subject/topic.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub subject: Option<String>,
    /// Free-form text description.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "free-text", default, skip_serializing_if = "Option::is_none")
    )]
    pub free_text: Option<String>,
    /// Space-separated keywords.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub keywords: Option<String>,
    /// Access and participation URIs.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "conf-uris", default, skip_serializing_if = "Option::is_none")
    )]
    pub conf_uris: Option<Uris>,
    /// Auxiliary service URIs (web page, streaming, recording).
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "service-uris",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub service_uris: Option<Uris>,
    /// Maximum number of participants allowed.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "maximum-user-count",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub maximum_user_count: Option<u32>,
    /// Available media types for this conference.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "available-media",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub available_media: Option<AvailableMedia>,
}

/// Container for available media descriptions in a conference.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct AvailableMedia {
    /// Individual media type entries.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "entry", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub entries: Vec<MediaDescription>,
}

/// A media type available in the conference (part of conference-description).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct MediaDescription {
    /// Media label attribute.
    #[cfg_attr(feature = "serde", serde(rename = "@label", default))]
    pub label: String,
    /// Human-readable description.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// SDP media type (audio, video, text).
    #[cfg_attr(
        feature = "serde",
        serde(rename = "type", default, skip_serializing_if = "Option::is_none")
    )]
    pub media_type: Option<String>,
    /// Directionality/status.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub status: Option<MediaStatus>,
}

impl MediaDescription {
    /// Create a media description with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            display_text: None,
            media_type: None,
            status: None,
        }
    }
}

/// Conference host information (RFC 4575 Section 5.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct HostInfo {
    /// Host display name.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// Host web page URL.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "web-page", default, skip_serializing_if = "Option::is_none")
    )]
    pub web_page: Option<String>,
    /// Host contact URIs.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub uris: Option<Uris>,
}

/// Aggregate conference state (RFC 4575 Section 5.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct ConferenceState {
    /// Number of participants currently in the conference.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "user-count",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub user_count: Option<u32>,
    /// Whether the conference is actively in session.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub active: Option<bool>,
    /// Whether the conference is locked for new participants.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub locked: Option<bool>,
}

/// Container for URI entries, used by host-info, conf-uris, service-uris,
/// associated-aors, and sidebars-by-ref.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Uris {
    /// Partial notification state.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@state", default, skip_serializing_if = "Option::is_none")
    )]
    pub state: Option<State>,
    /// Individual URI entries.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "entry", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub entries: Vec<UriEntry>,
}

/// A single URI entry with optional metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct UriEntry {
    /// The URI value.
    #[cfg_attr(feature = "serde", serde(default))]
    pub uri: String,
    /// Human-readable label.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// Entry purpose (e.g. "participation", "streaming", "web-page").
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub purpose: Option<String>,
    /// Last modification timestamp.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub modified: Option<String>,
}

impl UriEntry {
    /// Create a URI entry.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            display_text: None,
            purpose: None,
            modified: None,
        }
    }
}

/// Container for inline sidebar conferences (RFC 4575 Section 5.8).
///
/// Each entry is a full [`ConferenceInfo`](super::ConferenceInfo) document,
/// allowing recursive nesting.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct SidebarsByVal {
    /// Partial notification state.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@state", default, skip_serializing_if = "Option::is_none")
    )]
    pub state: Option<State>,
    /// Inline conference-info entries.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "entry", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub entries: Vec<super::ConferenceInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_entry_new() {
        let entry = UriEntry::new("sip:conf@example.com");
        assert_eq!(entry.uri, "sip:conf@example.com");
        assert!(entry
            .display_text
            .is_none());
    }

    #[test]
    fn conference_state_default() {
        let state = ConferenceState::default();
        assert!(state
            .user_count
            .is_none());
        assert!(state
            .active
            .is_none());
        assert!(state
            .locked
            .is_none());
    }
}
