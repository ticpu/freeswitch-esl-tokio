//! RFC 4575 `application/conference-info+xml` types and parsing.
//!
//! Models the conference event package defined in
//! [RFC 4575](https://www.rfc-editor.org/rfc/rfc4575) for SIP NOTIFY bodies.
//! Used by FreeSWITCH mod_conference, NG911 BCF systems, and any SIP
//! conference focus that publishes participant state.
//!
//! # Parsing
//!
//! With the `conference-info` feature enabled, call `ConferenceInfo::from_xml`
//! to deserialize a conference-info XML document. Namespace prefixes are
//! normalized automatically — documents using `confInfo:`, `ci:`, or the
//! default namespace all parse identically.
//!
//! ```rust,ignore
//! use freeswitch_types::conference_info::ConferenceInfo;
//!
//! let doc = ConferenceInfo::from_xml(xml_body)?;
//! if let Some(users) = &doc.users {
//!     for user in &users.users {
//!         println!("{}: {} endpoints", user.entity, user.endpoints.len());
//!     }
//! }
//! ```

/// Shared primitives: [`State`], [`ExecutionInfo`].
pub mod common;
/// Endpoint types: [`Endpoint`], [`EndpointStatus`], [`CallInfo`], [`SipDialogId`].
pub mod endpoint;
/// Error type for XML parsing.
pub mod error;
/// Media stream types: [`Media`], [`MediaStatus`].
pub mod media;
#[cfg(feature = "conference-info")]
mod normalize;
/// Conference metadata: [`ConferenceDescription`], [`HostInfo`], [`ConferenceState`], [`Uris`].
pub mod types;
/// Participant types: [`Users`], [`User`].
pub mod user;

pub use common::{ExecutionInfo, ParseStateError, State};
pub use endpoint::{
    CallInfo, DisconnectionMethod, Endpoint, EndpointStatus, JoiningMethod,
    ParseDisconnectionMethodError, ParseEndpointStatusError, ParseJoiningMethodError, SipDialogId,
};
pub use error::ConferenceInfoError;
pub use media::{Media, MediaStatus, ParseMediaStatusError};
pub use types::{
    AvailableMedia, ConferenceDescription, ConferenceState, HostInfo, MediaDescription,
    SidebarsByVal, UriEntry, Uris,
};
pub use user::{User, UserRoles, Users};

/// Root element of an RFC 4575 conference-info document.
///
/// The `entity` URI identifies the conference. The `state` attribute
/// indicates whether this is a full snapshot or a partial delta update.
/// The `version` counter increases monotonically across notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename = "conference-info"))]
#[non_exhaustive]
pub struct ConferenceInfo {
    /// Conference URI (required).
    #[cfg_attr(feature = "serde", serde(rename = "@entity"))]
    pub entity: String,
    /// Notification type: full snapshot, partial delta, or deletion.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@state", default, skip_serializing_if = "Option::is_none")
    )]
    pub state: Option<State>,
    /// Monotonically increasing version counter.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "@version", default, skip_serializing_if = "Option::is_none")
    )]
    pub version: Option<u32>,
    /// Conference metadata.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "conference-description",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub conference_description: Option<ConferenceDescription>,
    /// Hosting entity information.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "host-info", default, skip_serializing_if = "Option::is_none")
    )]
    pub host_info: Option<HostInfo>,
    /// Aggregate conference state (participant count, locked, active).
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "conference-state",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub conference_state: Option<ConferenceState>,
    /// Participant list.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub users: Option<Users>,
    /// Referenced sidebar conference URIs.
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "sidebars-by-ref",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub sidebars_by_ref: Option<Uris>,
    /// Inline sidebar conferences (recursive).
    #[cfg_attr(
        feature = "serde",
        serde(
            rename = "sidebars-by-val",
            default,
            skip_serializing_if = "Option::is_none"
        )
    )]
    pub sidebars_by_val: Option<SidebarsByVal>,
}

impl ConferenceInfo {
    /// Create a conference-info document with the given entity URI.
    pub fn new(entity: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            state: None,
            version: None,
            conference_description: None,
            host_info: None,
            conference_state: None,
            users: None,
            sidebars_by_ref: None,
            sidebars_by_val: None,
        }
    }

    /// Set the notification state.
    pub fn with_state(mut self, state: State) -> Self {
        self.state = Some(state);
        self
    }

    /// Set the version counter.
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    /// Set the users container.
    pub fn with_users(mut self, users: Users) -> Self {
        self.users = Some(users);
        self
    }

    /// Set the conference state.
    pub fn with_conference_state(mut self, state: ConferenceState) -> Self {
        self.conference_state = Some(state);
        self
    }

    /// Set the host info.
    pub fn with_host_info(mut self, host_info: HostInfo) -> Self {
        self.host_info = Some(host_info);
        self
    }

    /// Parse an RFC 4575 conference-info XML document.
    ///
    /// Handles any namespace prefix (`confInfo:`, `ci:`, or default namespace)
    /// by normalizing the XML before deserialization.
    #[cfg(feature = "conference-info")]
    pub fn from_xml(xml: &str) -> Result<Self, ConferenceInfoError> {
        let normalized = normalize::strip_namespace_prefixes(xml)?;
        quick_xml::de::from_str(&normalized).map_err(|e| ConferenceInfoError::Xml(e.to_string()))
    }

    /// Serialize to XML (without namespace prefix).
    #[cfg(feature = "conference-info")]
    pub fn to_xml(&self) -> Result<String, ConferenceInfoError> {
        quick_xml::se::to_string(self).map_err(|e| ConferenceInfoError::Xml(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_pattern() {
        let doc = ConferenceInfo::new("sip:conf@example.com")
            .with_state(State::Full)
            .with_version(1)
            .with_users(Users::new().with_user(User::new("sip:alice@example.com")));
        assert_eq!(doc.entity, "sip:conf@example.com");
        assert_eq!(doc.state, Some(State::Full));
        assert_eq!(doc.version, Some(1));
        assert_eq!(
            doc.users
                .as_ref()
                .unwrap()
                .users
                .len(),
            1
        );
    }

    #[cfg(feature = "conference-info")]
    mod xml {
        use super::*;

        const PREFIXED_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><confInfo:conference-info entity="sip:844c390d67e44b309e02e5b78bda695f@focus.qc.core.ng.911bell.ca" state="full" version="3" xmlns:confInfo="urn:ietf:params:xml:ns:conference-info"><confInfo:host-info><confInfo:display-text>sip:418100911.cauca02amb.qc.lsrg@lsrg.qc.core.ng.911bell.ca</confInfo:display-text><confInfo:uris><confInfo:entry><confInfo:uri>sip:418100911.cauca02amb.qc.lsrg@lsrg.qc.core.ng.911bell.ca</confInfo:uri></confInfo:entry><confInfo:entry><confInfo:uri>sip:418100911.cauca02amb.qc.lsrg@sip.bcf.qc.core.ng.911bell.ca;participantid=34a0c8fb87ad4dedaf660119531ff4ec</confInfo:uri></confInfo:entry></confInfo:uris></confInfo:host-info><confInfo:conference-state><confInfo:user-count>3</confInfo:user-count></confInfo:conference-state><confInfo:users><confInfo:user entity="sip:1251@cauca1.qc.psap.ng.911bell.ca" state="full"><confInfo:display-text></confInfo:display-text><confInfo:endpoint entity="sip:1251@sip.bcf.qc.core.ng.911bell.ca;participantid=b57ad3703be34cb0a7be6875c54029de" state="full"><confInfo:display-text></confInfo:display-text><confInfo:status>connected</confInfo:status><confInfo:joining-method>dialed-in</confInfo:joining-method><confInfo:joining-info><confInfo:when>2026-02-24T14:26:16.217046Z</confInfo:when></confInfo:joining-info><confInfo:media><confInfo:type>audio</confInfo:type><confInfo:status>sendrecv</confInfo:status></confInfo:media><confInfo:call-info><confInfo:sip><confInfo:call-id>7b1ee8b0-8c2f-123f-9288-52540077f337</confInfo:call-id><confInfo:from-tag>s891046-7019203084996215911</confInfo:from-tag><confInfo:to-tag>aj0m7ycy0pmZD</confInfo:to-tag></confInfo:sip></confInfo:call-info></confInfo:endpoint></confInfo:user><confInfo:user entity="sip:+14183699029@10.90.123.1" state="full"><confInfo:endpoint entity="sip:+14183699029@sip.bcf.qc.core.ng.911bell.ca;participantid=8d4dd59ea6fe47d688028fab7c8fa5d1" state="full"><confInfo:status>connected</confInfo:status><confInfo:joining-method>dialed-out</confInfo:joining-method><confInfo:joining-info><confInfo:when>2026-02-24T14:26:16.555847Z</confInfo:when><confInfo:by>sip:1251@sip.bcf.qc.core.ng.911bell.ca;participantid=b57ad3703be34cb0a7be6875c54029de</confInfo:by></confInfo:joining-info><confInfo:media><confInfo:type>audio</confInfo:type><confInfo:status>sendrecv</confInfo:status></confInfo:media><confInfo:call-info><confInfo:sip><confInfo:call-id>s891046-7253823020658607189@10.90.123.16</confInfo:call-id><confInfo:from-tag>s891046-7253823020658607189</confInfo:from-tag><confInfo:to-tag>o1g210gloh000</confInfo:to-tag></confInfo:sip></confInfo:call-info></confInfo:endpoint></confInfo:user><confInfo:user entity="sip:418100911.cauca02amb.qc.lsrg@lsrg.qc.core.ng.911bell.ca" state="full"><confInfo:endpoint entity="sip:418100911.cauca02amb.qc.lsrg@sip.bcf.qc.core.ng.911bell.ca;participantid=34a0c8fb87ad4dedaf660119531ff4ec" state="full"><confInfo:status>connected</confInfo:status><confInfo:joining-method>dialed-out</confInfo:joining-method><confInfo:joining-info><confInfo:when>2026-02-24T14:26:16.719118Z</confInfo:when><confInfo:by>sip:1251@sip.bcf.qc.core.ng.911bell.ca;participantid=b57ad3703be34cb0a7be6875c54029de</confInfo:by></confInfo:joining-info><confInfo:media><confInfo:type>audio</confInfo:type><confInfo:status>sendrecv</confInfo:status></confInfo:media><confInfo:call-info><confInfo:sip><confInfo:call-id>s891046-9031089336509078017@10.90.123.16</confInfo:call-id><confInfo:from-tag>s891046-9031089336509078017</confInfo:from-tag><confInfo:to-tag>s2517914-7595433079111460940</confInfo:to-tag></confInfo:sip></confInfo:call-info></confInfo:endpoint></confInfo:user></confInfo:users></confInfo:conference-info>"#;

        #[test]
        fn parse_prefixed_production_xml() {
            let doc = ConferenceInfo::from_xml(PREFIXED_XML).unwrap();
            assert_eq!(
                doc.entity,
                "sip:844c390d67e44b309e02e5b78bda695f@focus.qc.core.ng.911bell.ca"
            );
            assert_eq!(doc.state, Some(State::Full));
            assert_eq!(doc.version, Some(3));

            // host-info
            let host = doc
                .host_info
                .as_ref()
                .unwrap();
            assert_eq!(
                host.display_text
                    .as_deref(),
                Some("sip:418100911.cauca02amb.qc.lsrg@lsrg.qc.core.ng.911bell.ca")
            );
            let host_uris = host
                .uris
                .as_ref()
                .unwrap();
            assert_eq!(
                host_uris
                    .entries
                    .len(),
                2
            );

            // conference-state
            let conf_state = doc
                .conference_state
                .as_ref()
                .unwrap();
            assert_eq!(conf_state.user_count, Some(3));

            // users
            let users = doc
                .users
                .as_ref()
                .unwrap();
            assert_eq!(
                users
                    .users
                    .len(),
                3
            );

            // first user (PSAP operator)
            let psap = &users.users[0];
            assert_eq!(psap.entity, "sip:1251@cauca1.qc.psap.ng.911bell.ca");
            assert_eq!(
                psap.endpoints
                    .len(),
                1
            );
            let ep = &psap.endpoints[0];
            assert_eq!(ep.status, Some(EndpointStatus::Connected));
            assert_eq!(ep.joining_method, Some(JoiningMethod::DialedIn));
            assert_eq!(
                ep.joining_info
                    .as_ref()
                    .unwrap()
                    .when
                    .as_deref(),
                Some("2026-02-24T14:26:16.217046Z")
            );

            // media
            assert_eq!(
                ep.media
                    .len(),
                1
            );
            assert_eq!(
                ep.media[0]
                    .media_type
                    .as_deref(),
                Some("audio")
            );
            assert_eq!(ep.media[0].status, Some(MediaStatus::SendRecv));

            // call-info
            let sip = ep
                .call_info
                .as_ref()
                .unwrap()
                .sip
                .as_ref()
                .unwrap();
            assert_eq!(
                sip.call_id
                    .as_deref(),
                Some("7b1ee8b0-8c2f-123f-9288-52540077f337")
            );
            assert_eq!(
                sip.from_tag
                    .as_deref(),
                Some("s891046-7019203084996215911")
            );
            assert_eq!(
                sip.to_tag
                    .as_deref(),
                Some("aj0m7ycy0pmZD")
            );

            // second user (caller, dialed-out)
            let caller = &users.users[1];
            assert_eq!(caller.entity, "sip:+14183699029@10.90.123.1");
            assert_eq!(
                caller.endpoints[0].joining_method,
                Some(JoiningMethod::DialedOut)
            );
            assert!(caller.endpoints[0]
                .joining_info
                .as_ref()
                .unwrap()
                .by
                .is_some());
        }

        #[test]
        fn parse_unprefixed_xml() {
            let xml = r#"<conference-info xmlns="urn:ietf:params:xml:ns:conference-info" entity="sip:conf@example.com" state="full" version="1"><conference-state><user-count>2</user-count></conference-state><users><user entity="sip:alice@example.com" state="full"><endpoint entity="sip:alice@pc1" state="full"><status>connected</status><joining-method>dialed-in</joining-method><media><type>audio</type><status>sendrecv</status></media></endpoint></user></users></conference-info>"#;
            let doc = ConferenceInfo::from_xml(xml).unwrap();
            assert_eq!(doc.entity, "sip:conf@example.com");
            assert_eq!(
                doc.conference_state
                    .as_ref()
                    .unwrap()
                    .user_count,
                Some(2)
            );
            let users = doc
                .users
                .as_ref()
                .unwrap();
            assert_eq!(
                users
                    .users
                    .len(),
                1
            );
            assert_eq!(users.users[0].entity, "sip:alice@example.com");
        }

        #[test]
        fn parse_ci_prefix() {
            let xml = r#"<ci:conference-info xmlns:ci="urn:ietf:params:xml:ns:conference-info" entity="sip:test@example.com" state="partial" version="5"><ci:conference-state><ci:user-count>1</ci:user-count><ci:active>true</ci:active><ci:locked>false</ci:locked></ci:conference-state></ci:conference-info>"#;
            let doc = ConferenceInfo::from_xml(xml).unwrap();
            assert_eq!(doc.entity, "sip:test@example.com");
            assert_eq!(doc.state, Some(State::Partial));
            assert_eq!(doc.version, Some(5));
            let cs = doc
                .conference_state
                .as_ref()
                .unwrap();
            assert_eq!(cs.user_count, Some(1));
            assert_eq!(cs.active, Some(true));
            assert_eq!(cs.locked, Some(false));
        }

        #[test]
        fn parse_minimal() {
            let xml = r#"<conference-info entity="sip:minimal@example.com"></conference-info>"#;
            let doc = ConferenceInfo::from_xml(xml).unwrap();
            assert_eq!(doc.entity, "sip:minimal@example.com");
            assert!(doc
                .state
                .is_none());
            assert!(doc
                .version
                .is_none());
            assert!(doc
                .users
                .is_none());
        }

        #[test]
        fn malformed_xml_returns_error() {
            let result = ConferenceInfo::from_xml("<not-valid-xml");
            assert!(result.is_err());
        }

        #[test]
        fn round_trip() {
            let doc = ConferenceInfo::from_xml(PREFIXED_XML).unwrap();
            let xml_out = doc
                .to_xml()
                .unwrap();
            let doc2 = ConferenceInfo::from_xml(&xml_out).unwrap();
            assert_eq!(doc, doc2);
        }
    }
}
