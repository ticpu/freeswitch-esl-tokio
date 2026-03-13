//! Channel variable types: format parsers (`ARRAY::`, SIP multipart) and typed
//! variable name enums.

mod core;
mod esl_array;
mod sip_call_info;
mod sip_geolocation;
mod sip_history_info;
mod sip_invite;
mod sip_multipart;
mod sofia;

/// Split comma-separated entries respecting angle-bracket nesting.
pub(crate) fn split_comma_entries(raw: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;

    for (i, ch) in raw.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(&raw[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < raw.len() {
        entries.push(&raw[start..]);
    }

    entries
}

pub use self::core::{ChannelVariable, ParseChannelVariableError};
pub use esl_array::EslArray;
pub use sip_call_info::{SipCallInfo, SipCallInfoEntry, SipCallInfoError};
pub use sip_geolocation::{SipGeolocation, SipGeolocationRef};
pub use sip_history_info::{HistoryInfo, HistoryInfoEntry, HistoryInfoError, HistoryInfoReason};

pub use sip_invite::{ParseSipInviteHeaderError, SipInviteHeader};
pub use sip_multipart::{MultipartBody, MultipartItem};
pub use sofia::{ParseSofiaVariableError, SofiaVariable};

/// Trait for typed channel variable name enums.
///
/// Implement this on variable name enums to use them with
/// [`HeaderLookup::variable()`](crate::HeaderLookup::variable) and
/// [`variable_str()`](crate::HeaderLookup::variable_str).
/// For variables not covered by any typed enum, use `variable_str()`.
pub trait VariableName {
    /// Wire-format variable name (e.g. `"sip_call_id"`).
    fn as_str(&self) -> &str;
}

impl VariableName for ChannelVariable {
    fn as_str(&self) -> &str {
        ChannelVariable::as_str(self)
    }
}

impl VariableName for SofiaVariable {
    fn as_str(&self) -> &str {
        SofiaVariable::as_str(self)
    }
}

impl VariableName for SipInviteHeader {
    fn as_str(&self) -> &str {
        SipInviteHeader::as_str(self)
    }
}
