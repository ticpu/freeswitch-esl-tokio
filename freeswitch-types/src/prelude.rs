//! Convenience re-exports for common types and traits.
//!
//! ```
//! use freeswitch_types::prelude::*;
//! ```
//!
//! This brings [`HeaderLookup`] into scope (required for typed accessors like
//! `unique_id()`, `channel_state()`, `hangup_cause()`, etc.) along with the
//! header and variable enums used with `header()` and `variable()`.

pub use crate::headers::EventHeader;
pub use crate::lookup::HeaderLookup;
pub use crate::sip_header::{SipHeader, SipHeaderLookup};
pub use crate::variables::{ChannelVariable, VariableName};
