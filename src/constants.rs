//! Protocol constants and configuration values

/// Default FreeSWITCH ESL port for inbound connections
pub const DEFAULT_ESL_PORT: u16 = 8021;

/// Socket buffer size for reading from TCP stream (64KB) - standard TCP receive window
pub const SOCKET_BUF_SIZE: usize = 65536;

/// Buffer allocation size (64KB) - used for both initial allocation and growth increments
/// Handles 99% of ESL messages without reallocation
pub const BUF_CHUNK: usize = 64 * 1024;

/// Maximum single message size (8MB) - validates Content-Length header
/// No legitimate ESL message should exceed this (largest is sofia status ~1-2MB)
pub const MAX_MESSAGE_SIZE: usize = 8 * 1024 * 1024;

/// Maximum total buffer size (16MB) - safety limit to prevent runaway memory
/// Should hold 2 max messages + overhead. Indicates a bug if exceeded.
pub const MAX_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Protocol message terminators
pub const HEADER_TERMINATOR: &str = "\n\n";
pub const LINE_TERMINATOR: &str = "\n";

/// Content-Type header values
pub const CONTENT_TYPE_AUTH_REQUEST: &str = "auth/request";
pub const CONTENT_TYPE_COMMAND_REPLY: &str = "command/reply";
pub const CONTENT_TYPE_API_RESPONSE: &str = "api/response";
pub const CONTENT_TYPE_TEXT_EVENT_PLAIN: &str = "text/event-plain";
pub const CONTENT_TYPE_TEXT_EVENT_JSON: &str = "text/event-json";
pub const CONTENT_TYPE_TEXT_EVENT_XML: &str = "text/event-xml";

/// Protocol framing header names (not event payload — these stay as constants).
pub const HEADER_CONTENT_TYPE: &str = "Content-Type";
/// Protocol framing header: body length.
pub const HEADER_CONTENT_LENGTH: &str = "Content-Length";
/// Protocol framing header: command reply status.
pub const HEADER_REPLY_TEXT: &str = "Reply-Text";

/// Use [`EventHeader::EventName`](crate::headers::EventHeader::EventName) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::EventName")]
pub const HEADER_EVENT_NAME: &str = "Event-Name";
/// Use [`EventHeader::UniqueId`](crate::headers::EventHeader::UniqueId) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::UniqueId")]
pub const HEADER_UNIQUE_ID: &str = "Unique-ID";
/// Use [`EventHeader::CallerUniqueId`](crate::headers::EventHeader::CallerUniqueId) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::CallerUniqueId")]
pub const HEADER_CALLER_UUID: &str = "Caller-Unique-ID";
/// Use [`EventHeader::JobUuid`](crate::headers::EventHeader::JobUuid) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::JobUuid")]
pub const HEADER_JOB_UUID: &str = "Job-UUID";

/// Use [`EventHeader::ChannelState`](crate::headers::EventHeader::ChannelState) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::ChannelState")]
pub const HEADER_CHANNEL_STATE: &str = "Channel-State";
/// Use [`EventHeader::ChannelStateNumber`](crate::headers::EventHeader::ChannelStateNumber) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::ChannelStateNumber")]
pub const HEADER_CHANNEL_STATE_NUMBER: &str = "Channel-State-Number";
/// Use [`EventHeader::ChannelCallState`](crate::headers::EventHeader::ChannelCallState) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::ChannelCallState")]
pub const HEADER_CHANNEL_CALL_STATE: &str = "Channel-Call-State";
/// Use [`EventHeader::AnswerState`](crate::headers::EventHeader::AnswerState) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::AnswerState")]
pub const HEADER_ANSWER_STATE: &str = "Answer-State";
/// Use [`EventHeader::CallDirection`](crate::headers::EventHeader::CallDirection) instead.
#[deprecated(since = "1.2.0", note = "use EventHeader::CallDirection")]
pub const HEADER_CALL_DIRECTION: &str = "Call-Direction";

/// Connection timeout in milliseconds
pub const DEFAULT_TIMEOUT_MS: u64 = 2000;

/// Maximum number of queued events before dropping
pub const MAX_EVENT_QUEUE_SIZE: usize = 1000;
