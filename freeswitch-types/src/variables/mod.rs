//! Channel variable types: format parsers (`ARRAY::`, SIP multipart) and typed
//! variable name enums.

mod core;
mod esl_array;
mod sip_call_info;
mod sip_geolocation;
mod sip_header_addr;
mod sip_invite;
mod sip_multipart;
mod sofia;

pub use self::core::{ChannelVariable, ParseChannelVariableError};
pub use esl_array::EslArray;
pub use sip_call_info::{SipCallInfo, SipCallInfoEntry, SipCallInfoError};
pub use sip_geolocation::{SipGeolocation, SipGeolocationRef};
pub use sip_header_addr::{ParseSipHeaderAddrError, SipHeaderAddr};
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
