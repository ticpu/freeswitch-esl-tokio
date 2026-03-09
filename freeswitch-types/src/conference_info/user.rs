use super::common::State;
use super::endpoint::Endpoint;

/// Container for conference participants (RFC 4575 Section 5.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Users {
    /// Partial notification state for the user list.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@state", default, skip_serializing_if = "Option::is_none")
    )]
    pub state: Option<State>,
    /// Individual participant entries.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "user", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub users: Vec<User>,
}

impl Users {
    /// Create an empty users container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a user.
    pub fn with_user(mut self, user: User) -> Self {
        self.users
            .push(user);
        self
    }
}

/// A conference participant (RFC 4575 Section 5.4).
///
/// A user represents a logical identity (SIP AOR) and may have multiple
/// endpoints (devices) connected simultaneously.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct User {
    /// Logical user SIP URI (required XML attribute).
    #[cfg_attr(feature = "serde", serde(rename = "@entity"))]
    pub entity: String,
    /// Partial notification state.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@state", default, skip_serializing_if = "Option::is_none")
    )]
    pub state: Option<State>,
    /// Human-readable name or extension number.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "display-text",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub display_text: Option<String>,
    /// Alternative addresses of record (GRUUs, tel: URIs).
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "associated-aors",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub associated_aors: Option<super::types::Uris>,
    /// Participant roles (e.g. "participant", "organizer").
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub roles: Option<UserRoles>,
    /// Preferred languages per RFC 3066.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub languages: Option<String>,
    /// Focus URI for cascaded conference scenarios.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "cascaded-focus",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub cascaded_focus: Option<String>,
    /// Connected devices/sessions for this user.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "endpoint", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub endpoints: Vec<Endpoint>,
}

impl User {
    /// Create a user with the given entity URI.
    pub fn new(entity: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            state: None,
            display_text: None,
            associated_aors: None,
            roles: None,
            languages: None,
            cascaded_focus: None,
            endpoints: Vec::new(),
        }
    }

    /// Set the display text.
    pub fn with_display_text(mut self, text: impl Into<String>) -> Self {
        self.display_text = Some(text.into());
        self
    }

    /// Add an endpoint.
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoints
            .push(endpoint);
        self
    }
}

/// Container for user role entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct UserRoles {
    /// Role name entries (e.g. "participant", "organizer").
    #[cfg_attr(
        feature = "serde",
        serde(rename = "entry", default, skip_serializing_if = "Vec::is_empty")
    )]
    pub entries: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conference_info::endpoint::{Endpoint, EndpointStatus};

    #[test]
    fn user_builder() {
        let user = User::new("sip:alice@example.com")
            .with_display_text("Alice")
            .with_endpoint(
                Endpoint::new("sip:alice@pc1.example.com").with_status(EndpointStatus::Connected),
            );
        assert_eq!(user.entity, "sip:alice@example.com");
        assert_eq!(
            user.display_text
                .as_deref(),
            Some("Alice")
        );
        assert_eq!(
            user.endpoints
                .len(),
            1
        );
    }

    #[test]
    fn users_container() {
        let users = Users::new()
            .with_user(User::new("sip:alice@example.com"))
            .with_user(User::new("sip:bob@example.com"));
        assert_eq!(
            users
                .users
                .len(),
            2
        );
    }
}
