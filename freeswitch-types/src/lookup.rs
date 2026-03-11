//! Shared trait for typed header lookups from any key-value store.
//!
//! [`HeaderLookup`] provides convenience accessors (typed channel state,
//! call direction, timetable extraction, etc.) to any type that can look up
//! headers and variables by name. Implement the two required methods and get
//! everything else for free.

use crate::channel::{
    AnswerState, CallDirection, CallState, ChannelState, ChannelTimetable, HangupCause,
    ParseAnswerStateError, ParseCallDirectionError, ParseCallStateError, ParseChannelStateError,
    ParseHangupCauseError, ParseTimetableError,
};
#[cfg(feature = "esl")]
use crate::event::{EslEventPriority, ParsePriorityError};
use crate::headers::EventHeader;
use crate::variables::VariableName;

/// Trait for looking up ESL headers and channel variables from any key-value store.
///
/// Implementors provide two methods — `header_str(&str)` and `variable_str(&str)` —
/// and get all typed accessors (`channel_state()`, `call_direction()`, `timetable()`,
/// etc.) as default implementations.
///
/// This trait must be in scope to call its methods on `EslEvent` -- including
/// `unique_id()`, `hangup_cause()`, and `channel_state()`. Import it directly
/// or via the prelude:
///
/// ```ignore
/// use freeswitch_esl_tokio::prelude::*;
/// // or: use freeswitch_esl_tokio::HeaderLookup;
/// // or: use freeswitch_types::HeaderLookup;
/// ```
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use freeswitch_types::{HeaderLookup, EventHeader, ChannelVariable};
///
/// struct MyStore(HashMap<String, String>);
///
/// impl HeaderLookup for MyStore {
///     fn header_str(&self, name: &str) -> Option<&str> {
///         self.0.get(name).map(|s| s.as_str())
///     }
///     fn variable_str(&self, name: &str) -> Option<&str> {
///         self.0.get(&format!("variable_{}", name)).map(|s| s.as_str())
///     }
/// }
///
/// let mut map = HashMap::new();
/// map.insert("Channel-State".into(), "CS_EXECUTE".into());
/// map.insert("variable_read_codec".into(), "PCMU".into());
/// let store = MyStore(map);
///
/// // Typed accessor from the trait (returns Result<Option<T>, E>):
/// assert!(store.channel_state().unwrap().is_some());
///
/// // Enum-based lookups:
/// assert_eq!(store.header(EventHeader::ChannelState), Some("CS_EXECUTE"));
/// assert_eq!(store.variable(ChannelVariable::ReadCodec), Some("PCMU"));
/// ```
pub trait HeaderLookup {
    /// Look up a header by its raw wire name (e.g. `"Unique-ID"`).
    fn header_str(&self, name: &str) -> Option<&str>;

    /// Look up a channel variable by its bare name (e.g. `"sip_call_id"`).
    ///
    /// Implementations typically prepend `variable_` and delegate to `header_str`.
    fn variable_str(&self, name: &str) -> Option<&str>;

    /// Look up a header by its [`EventHeader`] enum variant.
    fn header(&self, name: EventHeader) -> Option<&str> {
        self.header_str(name.as_str())
    }

    /// Look up a channel variable by its typed enum variant.
    fn variable(&self, name: impl VariableName) -> Option<&str> {
        self.variable_str(name.as_str())
    }

    /// `Unique-ID` header, falling back to `Caller-Unique-ID`.
    fn unique_id(&self) -> Option<&str> {
        self.header(EventHeader::UniqueId)
            .or_else(|| self.header(EventHeader::CallerUniqueId))
    }

    /// `Job-UUID` header from `bgapi` `BACKGROUND_JOB` events.
    fn job_uuid(&self) -> Option<&str> {
        self.header(EventHeader::JobUuid)
    }

    /// `Channel-Name` header (e.g. `sofia/internal/1000@domain`).
    fn channel_name(&self) -> Option<&str> {
        self.header(EventHeader::ChannelName)
    }

    /// `Caller-Caller-ID-Number` header.
    fn caller_id_number(&self) -> Option<&str> {
        self.header(EventHeader::CallerCallerIdNumber)
    }

    /// `Caller-Caller-ID-Name` header.
    fn caller_id_name(&self) -> Option<&str> {
        self.header(EventHeader::CallerCallerIdName)
    }

    /// `Caller-Destination-Number` header.
    fn destination_number(&self) -> Option<&str> {
        self.header(EventHeader::CallerDestinationNumber)
    }

    /// `Caller-Callee-ID-Number` header.
    fn callee_id_number(&self) -> Option<&str> {
        self.header(EventHeader::CallerCalleeIdNumber)
    }

    /// `Caller-Callee-ID-Name` header.
    fn callee_id_name(&self) -> Option<&str> {
        self.header(EventHeader::CallerCalleeIdName)
    }

    /// Parse the `Hangup-Cause` header into a [`HangupCause`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn hangup_cause(&self) -> Result<Option<HangupCause>, ParseHangupCauseError> {
        match self.header(EventHeader::HangupCause) {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    /// `Event-Subclass` header for `CUSTOM` events (e.g. `sofia::register`).
    fn event_subclass(&self) -> Option<&str> {
        self.header(EventHeader::EventSubclass)
    }

    /// `pl_data` header — SIP NOTIFY body content from `NOTIFY_IN` events.
    ///
    /// Contains the JSON payload (already percent-decoded by the ESL parser).
    /// For NG9-1-1 events this is the inner object without the wrapper key
    /// (FreeSWITCH strips it).
    fn pl_data(&self) -> Option<&str> {
        self.header(EventHeader::PlData)
    }

    /// `event` header — SIP event package name from `NOTIFY_IN` events.
    ///
    /// Examples: `emergency-AbandonedCall`, `emergency-ServiceState`.
    fn sip_event(&self) -> Option<&str> {
        self.header(EventHeader::SipEvent)
    }

    /// `gateway_name` header — gateway that received a SIP NOTIFY.
    fn gateway_name(&self) -> Option<&str> {
        self.header(EventHeader::GatewayName)
    }

    /// Parse the `Channel-State` header into a [`ChannelState`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn channel_state(&self) -> Result<Option<ChannelState>, ParseChannelStateError> {
        match self.header(EventHeader::ChannelState) {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    /// Parse the `Channel-State-Number` header into a [`ChannelState`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn channel_state_number(&self) -> Result<Option<ChannelState>, ParseChannelStateError> {
        match self.header(EventHeader::ChannelStateNumber) {
            Some(s) => {
                let n: u8 = s
                    .parse()
                    .map_err(|_| ParseChannelStateError(s.to_string()))?;
                ChannelState::from_number(n)
                    .ok_or_else(|| ParseChannelStateError(s.to_string()))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    /// Parse the `Channel-Call-State` header into a [`CallState`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn call_state(&self) -> Result<Option<CallState>, ParseCallStateError> {
        match self.header(EventHeader::ChannelCallState) {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    /// Parse the `Answer-State` header into an [`AnswerState`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn answer_state(&self) -> Result<Option<AnswerState>, ParseAnswerStateError> {
        match self.header(EventHeader::AnswerState) {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    /// Parse the `Call-Direction` header into a [`CallDirection`].
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    fn call_direction(&self) -> Result<Option<CallDirection>, ParseCallDirectionError> {
        match self.header(EventHeader::CallDirection) {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    /// Parse the `priority` header value.
    ///
    /// Returns `Ok(None)` if the header is absent, `Err` if present but unparseable.
    #[cfg(feature = "esl")]
    fn priority(&self) -> Result<Option<EslEventPriority>, ParsePriorityError> {
        match self.header(EventHeader::Priority) {
            Some(s) => Ok(Some(s.parse()?)),
            None => Ok(None),
        }
    }

    /// Extract timetable from timestamp headers with the given prefix.
    ///
    /// Returns `Ok(None)` if no timestamp headers with this prefix are present.
    /// Returns `Err` if a header is present but contains an invalid value.
    fn timetable(&self, prefix: &str) -> Result<Option<ChannelTimetable>, ParseTimetableError> {
        ChannelTimetable::from_lookup(prefix, |key| self.header_str(key))
    }

    /// Caller-leg channel timetable (`Caller-*-Time` headers).
    fn caller_timetable(&self) -> Result<Option<ChannelTimetable>, ParseTimetableError> {
        self.timetable("Caller")
    }

    /// Other-leg channel timetable (`Other-Leg-*-Time` headers).
    fn other_leg_timetable(&self) -> Result<Option<ChannelTimetable>, ParseTimetableError> {
        self.timetable("Other-Leg")
    }
}

impl HeaderLookup for std::collections::HashMap<String, String> {
    fn header_str(&self, name: &str) -> Option<&str> {
        self.get(name)
            .map(|s| s.as_str())
    }

    fn variable_str(&self, name: &str) -> Option<&str> {
        self.get(&format!("variable_{name}"))
            .map(|s| s.as_str())
    }
}

#[cfg(feature = "esl")]
impl HeaderLookup for indexmap::IndexMap<String, String> {
    fn header_str(&self, name: &str) -> Option<&str> {
        self.get(name)
            .map(|s| s.as_str())
    }

    fn variable_str(&self, name: &str) -> Option<&str> {
        self.get(&format!("variable_{name}"))
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variables::ChannelVariable;
    use std::collections::HashMap;

    struct TestStore(HashMap<String, String>);

    impl HeaderLookup for TestStore {
        fn header_str(&self, name: &str) -> Option<&str> {
            self.0
                .get(name)
                .map(|s| s.as_str())
        }
        fn variable_str(&self, name: &str) -> Option<&str> {
            self.0
                .get(&format!("variable_{}", name))
                .map(|s| s.as_str())
        }
    }

    fn store_with(pairs: &[(&str, &str)]) -> TestStore {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        TestStore(map)
    }

    #[test]
    fn header_str_direct() {
        let s = store_with(&[("Unique-ID", "abc-123")]);
        assert_eq!(s.header_str("Unique-ID"), Some("abc-123"));
        assert_eq!(s.header_str("Missing"), None);
    }

    #[test]
    fn header_by_enum() {
        let s = store_with(&[("Unique-ID", "abc-123")]);
        assert_eq!(s.header(EventHeader::UniqueId), Some("abc-123"));
    }

    #[test]
    fn variable_str_direct() {
        let s = store_with(&[("variable_read_codec", "PCMU")]);
        assert_eq!(s.variable_str("read_codec"), Some("PCMU"));
        assert_eq!(s.variable_str("missing"), None);
    }

    #[test]
    fn variable_by_enum() {
        let s = store_with(&[("variable_read_codec", "PCMU")]);
        assert_eq!(s.variable(ChannelVariable::ReadCodec), Some("PCMU"));
    }

    #[test]
    fn unique_id_primary() {
        let s = store_with(&[("Unique-ID", "uuid-1")]);
        assert_eq!(s.unique_id(), Some("uuid-1"));
    }

    #[test]
    fn unique_id_fallback() {
        let s = store_with(&[("Caller-Unique-ID", "uuid-2")]);
        assert_eq!(s.unique_id(), Some("uuid-2"));
    }

    #[test]
    fn unique_id_none() {
        let s = store_with(&[]);
        assert_eq!(s.unique_id(), None);
    }

    #[test]
    fn job_uuid() {
        let s = store_with(&[("Job-UUID", "job-1")]);
        assert_eq!(s.job_uuid(), Some("job-1"));
    }

    #[test]
    fn channel_name() {
        let s = store_with(&[("Channel-Name", "sofia/internal/1000@example.com")]);
        assert_eq!(s.channel_name(), Some("sofia/internal/1000@example.com"));
    }

    #[test]
    fn caller_id_number_and_name() {
        let s = store_with(&[
            ("Caller-Caller-ID-Number", "1000"),
            ("Caller-Caller-ID-Name", "Alice"),
        ]);
        assert_eq!(s.caller_id_number(), Some("1000"));
        assert_eq!(s.caller_id_name(), Some("Alice"));
    }

    #[test]
    fn hangup_cause_typed() {
        let s = store_with(&[("Hangup-Cause", "NORMAL_CLEARING")]);
        assert_eq!(
            s.hangup_cause()
                .unwrap(),
            Some(crate::channel::HangupCause::NormalClearing)
        );
    }

    #[test]
    fn hangup_cause_invalid_is_error() {
        let s = store_with(&[("Hangup-Cause", "BOGUS_CAUSE")]);
        assert!(s
            .hangup_cause()
            .is_err());
    }

    #[test]
    fn destination_number() {
        let s = store_with(&[("Caller-Destination-Number", "1000")]);
        assert_eq!(s.destination_number(), Some("1000"));
    }

    #[test]
    fn callee_id() {
        let s = store_with(&[
            ("Caller-Callee-ID-Number", "2000"),
            ("Caller-Callee-ID-Name", "Bob"),
        ]);
        assert_eq!(s.callee_id_number(), Some("2000"));
        assert_eq!(s.callee_id_name(), Some("Bob"));
    }

    #[test]
    fn event_subclass() {
        let s = store_with(&[("Event-Subclass", "sofia::register")]);
        assert_eq!(s.event_subclass(), Some("sofia::register"));
    }

    #[test]
    fn channel_state_typed() {
        let s = store_with(&[("Channel-State", "CS_EXECUTE")]);
        assert_eq!(
            s.channel_state()
                .unwrap(),
            Some(ChannelState::CsExecute)
        );
    }

    #[test]
    fn channel_state_number_typed() {
        let s = store_with(&[("Channel-State-Number", "4")]);
        assert_eq!(
            s.channel_state_number()
                .unwrap(),
            Some(ChannelState::CsExecute)
        );
    }

    #[test]
    fn call_state_typed() {
        let s = store_with(&[("Channel-Call-State", "ACTIVE")]);
        assert_eq!(
            s.call_state()
                .unwrap(),
            Some(CallState::Active)
        );
    }

    #[test]
    fn answer_state_typed() {
        let s = store_with(&[("Answer-State", "answered")]);
        assert_eq!(
            s.answer_state()
                .unwrap(),
            Some(AnswerState::Answered)
        );
    }

    #[test]
    fn call_direction_typed() {
        let s = store_with(&[("Call-Direction", "inbound")]);
        assert_eq!(
            s.call_direction()
                .unwrap(),
            Some(CallDirection::Inbound)
        );
    }

    #[test]
    fn priority_typed() {
        let s = store_with(&[("priority", "HIGH")]);
        assert_eq!(
            s.priority()
                .unwrap(),
            Some(EslEventPriority::High)
        );
    }

    #[test]
    fn timetable_extraction() {
        let s = store_with(&[
            ("Caller-Channel-Created-Time", "1700000001000000"),
            ("Caller-Channel-Answered-Time", "1700000005000000"),
        ]);
        let tt = s
            .caller_timetable()
            .unwrap()
            .expect("should have timetable");
        assert_eq!(tt.created, Some(1700000001000000));
        assert_eq!(tt.answered, Some(1700000005000000));
        assert_eq!(tt.hungup, None);
    }

    #[test]
    fn timetable_other_leg() {
        let s = store_with(&[("Other-Leg-Channel-Created-Time", "1700000001000000")]);
        let tt = s
            .other_leg_timetable()
            .unwrap()
            .expect("should have timetable");
        assert_eq!(tt.created, Some(1700000001000000));
    }

    #[test]
    fn timetable_none_when_absent() {
        let s = store_with(&[]);
        assert_eq!(
            s.caller_timetable()
                .unwrap(),
            None
        );
    }

    #[test]
    fn timetable_invalid_is_error() {
        let s = store_with(&[("Caller-Channel-Created-Time", "not_a_number")]);
        let err = s
            .caller_timetable()
            .unwrap_err();
        assert_eq!(err.header, "Caller-Channel-Created-Time");
    }

    #[test]
    fn missing_headers_return_none() {
        let s = store_with(&[]);
        assert_eq!(
            s.channel_state()
                .unwrap(),
            None
        );
        assert_eq!(
            s.channel_state_number()
                .unwrap(),
            None
        );
        assert_eq!(
            s.call_state()
                .unwrap(),
            None
        );
        assert_eq!(
            s.answer_state()
                .unwrap(),
            None
        );
        assert_eq!(
            s.call_direction()
                .unwrap(),
            None
        );
        assert_eq!(
            s.priority()
                .unwrap(),
            None
        );
        assert_eq!(
            s.hangup_cause()
                .unwrap(),
            None
        );
        assert_eq!(s.channel_name(), None);
        assert_eq!(s.caller_id_number(), None);
        assert_eq!(s.caller_id_name(), None);
        assert_eq!(s.destination_number(), None);
        assert_eq!(s.callee_id_number(), None);
        assert_eq!(s.callee_id_name(), None);
        assert_eq!(s.event_subclass(), None);
        assert_eq!(s.job_uuid(), None);
        assert_eq!(s.pl_data(), None);
        assert_eq!(s.sip_event(), None);
        assert_eq!(s.gateway_name(), None);
    }

    #[test]
    fn notify_in_headers() {
        let s = store_with(&[
            ("pl_data", r#"{"invite":"INVITE ..."}"#),
            ("event", "emergency-AbandonedCall"),
            ("gateway_name", "ng911-bcf"),
        ]);
        assert_eq!(s.pl_data(), Some(r#"{"invite":"INVITE ..."}"#));
        assert_eq!(s.sip_event(), Some("emergency-AbandonedCall"));
        assert_eq!(s.gateway_name(), Some("ng911-bcf"));
    }

    #[test]
    fn invalid_values_return_err() {
        let s = store_with(&[
            ("Channel-State", "BOGUS"),
            ("Channel-State-Number", "999"),
            ("Channel-Call-State", "BOGUS"),
            ("Answer-State", "bogus"),
            ("Call-Direction", "bogus"),
            ("priority", "BOGUS"),
            ("Hangup-Cause", "BOGUS"),
        ]);
        assert!(s
            .channel_state()
            .is_err());
        assert!(s
            .channel_state_number()
            .is_err());
        assert!(s
            .call_state()
            .is_err());
        assert!(s
            .answer_state()
            .is_err());
        assert!(s
            .call_direction()
            .is_err());
        assert!(s
            .priority()
            .is_err());
        assert!(s
            .hangup_cause()
            .is_err());
    }
}
