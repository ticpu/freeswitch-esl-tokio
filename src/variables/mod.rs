//! Channel variable types: format parsers (`ARRAY::`, SIP multipart) and typed
//! variable name enums.

mod channel_variable;
mod esl_array;
mod sip_multipart;

pub use channel_variable::{ChannelVariable, ParseChannelVariableError};
pub use esl_array::EslArray;
pub use sip_multipart::{MultipartBody, MultipartItem};
