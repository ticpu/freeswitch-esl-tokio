//! Typed Sofia event subclass names (`Event-Subclass` header values).

use std::fmt;
use std::str::FromStr;

/// Error returned when parsing an unrecognized Sofia event subclass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSofiaEventSubclassError(pub String);

impl fmt::Display for ParseSofiaEventSubclassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown sofia event subclass: {}", self.0)
    }
}

impl std::error::Error for ParseSofiaEventSubclassError {}

/// Sofia event subclass values from `mod_sofia.h` `MY_EVENT_*` defines.
///
/// These appear as the `Event-Subclass` header on `CUSTOM` events fired by
/// mod_sofia. Use with [`EventSubscription::sofia_event()`](crate::EventSubscription::sofia_event)
/// for typed subscriptions, or parse from received events via
/// [`HeaderLookup::sofia_event_subclass()`](crate::HeaderLookup::sofia_event_subclass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum SofiaEventSubclass {
    Register,
    PreRegister,
    RegisterAttempt,
    RegisterFailure,
    Unregister,
    Expire,
    GatewayState,
    SipUserState,
    NotifyRefer,
    Reinvite,
    GatewayAdd,
    GatewayDelete,
    GatewayInvalidDigestReq,
    RecoveryRecv,
    RecoverySend,
    RecoveryRecovered,
    Error,
    ProfileStart,
    NotifyWatchedHeader,
    WrongCallState,
    Transferor,
    Transferee,
    Replaced,
    Intercepted,
    ByeResponse,
}

impl SofiaEventSubclass {
    /// Wire-format string including the `sofia::` prefix.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Register => "sofia::register",
            Self::PreRegister => "sofia::pre_register",
            Self::RegisterAttempt => "sofia::register_attempt",
            Self::RegisterFailure => "sofia::register_failure",
            Self::Unregister => "sofia::unregister",
            Self::Expire => "sofia::expire",
            Self::GatewayState => "sofia::gateway_state",
            Self::SipUserState => "sofia::sip_user_state",
            Self::NotifyRefer => "sofia::notify_refer",
            Self::Reinvite => "sofia::reinvite",
            Self::GatewayAdd => "sofia::gateway_add",
            Self::GatewayDelete => "sofia::gateway_delete",
            Self::GatewayInvalidDigestReq => "sofia::gateway_invalid_digest_req",
            Self::RecoveryRecv => "sofia::recovery_recv",
            Self::RecoverySend => "sofia::recovery_send",
            Self::RecoveryRecovered => "sofia::recovery_recovered",
            Self::Error => "sofia::error",
            Self::ProfileStart => "sofia::profile_start",
            Self::NotifyWatchedHeader => "sofia::notify_watched_header",
            Self::WrongCallState => "sofia::wrong_call_state",
            Self::Transferor => "sofia::transferor",
            Self::Transferee => "sofia::transferee",
            Self::Replaced => "sofia::replaced",
            Self::Intercepted => "sofia::intercepted",
            Self::ByeResponse => "sofia::bye_response",
        }
    }

    /// All Sofia event subclass variants.
    pub const ALL: &[SofiaEventSubclass] = &[
        Self::Register,
        Self::PreRegister,
        Self::RegisterAttempt,
        Self::RegisterFailure,
        Self::Unregister,
        Self::Expire,
        Self::GatewayState,
        Self::SipUserState,
        Self::NotifyRefer,
        Self::Reinvite,
        Self::GatewayAdd,
        Self::GatewayDelete,
        Self::GatewayInvalidDigestReq,
        Self::RecoveryRecv,
        Self::RecoverySend,
        Self::RecoveryRecovered,
        Self::Error,
        Self::ProfileStart,
        Self::NotifyWatchedHeader,
        Self::WrongCallState,
        Self::Transferor,
        Self::Transferee,
        Self::Replaced,
        Self::Intercepted,
        Self::ByeResponse,
    ];

    /// Registration-related subclasses.
    pub const REGISTRATION_EVENTS: &[SofiaEventSubclass] = &[
        Self::Register,
        Self::PreRegister,
        Self::RegisterAttempt,
        Self::RegisterFailure,
        Self::Unregister,
        Self::Expire,
    ];

    /// Gateway lifecycle and state subclasses.
    pub const GATEWAY_EVENTS: &[SofiaEventSubclass] = &[
        Self::GatewayState,
        Self::GatewayAdd,
        Self::GatewayDelete,
        Self::GatewayInvalidDigestReq,
    ];
}

impl fmt::Display for SofiaEventSubclass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SofiaEventSubclass {
    type Err = ParseSofiaEventSubclassError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sofia::register" => Ok(Self::Register),
            "sofia::pre_register" => Ok(Self::PreRegister),
            "sofia::register_attempt" => Ok(Self::RegisterAttempt),
            "sofia::register_failure" => Ok(Self::RegisterFailure),
            "sofia::unregister" => Ok(Self::Unregister),
            "sofia::expire" => Ok(Self::Expire),
            "sofia::gateway_state" => Ok(Self::GatewayState),
            "sofia::sip_user_state" => Ok(Self::SipUserState),
            "sofia::notify_refer" => Ok(Self::NotifyRefer),
            "sofia::reinvite" => Ok(Self::Reinvite),
            "sofia::gateway_add" => Ok(Self::GatewayAdd),
            "sofia::gateway_delete" => Ok(Self::GatewayDelete),
            "sofia::gateway_invalid_digest_req" => Ok(Self::GatewayInvalidDigestReq),
            "sofia::recovery_recv" => Ok(Self::RecoveryRecv),
            "sofia::recovery_send" => Ok(Self::RecoverySend),
            "sofia::recovery_recovered" => Ok(Self::RecoveryRecovered),
            "sofia::error" => Ok(Self::Error),
            "sofia::profile_start" => Ok(Self::ProfileStart),
            "sofia::notify_watched_header" => Ok(Self::NotifyWatchedHeader),
            "sofia::wrong_call_state" => Ok(Self::WrongCallState),
            "sofia::transferor" => Ok(Self::Transferor),
            "sofia::transferee" => Ok(Self::Transferee),
            "sofia::replaced" => Ok(Self::Replaced),
            "sofia::intercepted" => Ok(Self::Intercepted),
            "sofia::bye_response" => Ok(Self::ByeResponse),
            _ => Err(ParseSofiaEventSubclassError(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_from_str_round_trip_all() {
        for &variant in SofiaEventSubclass::ALL {
            let wire = variant.to_string();
            assert!(wire.starts_with("sofia::"), "missing prefix: {wire}");
            let parsed: SofiaEventSubclass = wire
                .parse()
                .unwrap();
            assert_eq!(parsed, variant, "round-trip failed for {wire}");
        }
    }

    #[test]
    fn all_contains_every_variant() {
        assert_eq!(SofiaEventSubclass::ALL.len(), 25);
    }

    #[test]
    fn registration_events_subset() {
        for &ev in SofiaEventSubclass::REGISTRATION_EVENTS {
            assert!(SofiaEventSubclass::ALL.contains(&ev), "{ev} not in ALL");
        }
        assert_eq!(SofiaEventSubclass::REGISTRATION_EVENTS.len(), 6);
    }

    #[test]
    fn gateway_events_subset() {
        for &ev in SofiaEventSubclass::GATEWAY_EVENTS {
            assert!(SofiaEventSubclass::ALL.contains(&ev), "{ev} not in ALL");
        }
        assert_eq!(SofiaEventSubclass::GATEWAY_EVENTS.len(), 4);
    }

    #[test]
    fn from_str_rejects_unknown() {
        assert!("sofia::nonexistent"
            .parse::<SofiaEventSubclass>()
            .is_err());
        assert!("register"
            .parse::<SofiaEventSubclass>()
            .is_err());
        assert!(""
            .parse::<SofiaEventSubclass>()
            .is_err());
    }

    #[test]
    fn from_str_is_case_sensitive() {
        assert!("sofia::Register"
            .parse::<SofiaEventSubclass>()
            .is_err());
        assert!("SOFIA::REGISTER"
            .parse::<SofiaEventSubclass>()
            .is_err());
    }
}
