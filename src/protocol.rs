//! ESL protocol parsing and message handling

use crate::{
    buffer::EslBuffer,
    command::EslResponse,
    constants::{
        CONTENT_TYPE_API_RESPONSE, CONTENT_TYPE_AUTH_REQUEST, CONTENT_TYPE_COMMAND_REPLY,
        CONTENT_TYPE_LOG_DATA, CONTENT_TYPE_TEXT_EVENT_JSON, CONTENT_TYPE_TEXT_EVENT_PLAIN,
        CONTENT_TYPE_TEXT_EVENT_XML, HEADER_CONTENT_LENGTH, HEADER_CONTENT_TYPE, HEADER_TERMINATOR,
        MAX_MESSAGE_SIZE,
    },
    error::{EslError, EslResult},
    event::{EslEvent, EslEventType, EventFormat},
    headers::{normalize_header_key, EventHeader},
};
use indexmap::IndexMap;
use percent_encoding::percent_decode_str;

/// ESL message types
#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    /// Authentication request from server
    AuthRequest,
    /// Command reply
    CommandReply,
    /// API response
    ApiResponse,
    /// Event message
    Event,
    /// Disconnect notice
    Disconnect,
    /// Connection rejected by ACL (text/rude-rejection)
    RudeRejection,
    /// Unknown message type
    Unknown(String),
}

impl MessageType {
    /// Parse message type from Content-Type header
    pub fn from_content_type(content_type: &str) -> Self {
        match content_type {
            CONTENT_TYPE_AUTH_REQUEST => MessageType::AuthRequest,
            CONTENT_TYPE_COMMAND_REPLY => MessageType::CommandReply,
            CONTENT_TYPE_API_RESPONSE => MessageType::ApiResponse,
            CONTENT_TYPE_TEXT_EVENT_PLAIN
            | CONTENT_TYPE_TEXT_EVENT_JSON
            | CONTENT_TYPE_TEXT_EVENT_XML
            | CONTENT_TYPE_LOG_DATA => MessageType::Event,
            "text/disconnect-notice" => MessageType::Disconnect,
            "text/rude-rejection" => MessageType::RudeRejection,
            _ => MessageType::Unknown(content_type.to_string()),
        }
    }
}

/// Parsed ESL message
#[derive(Debug, Clone)]
pub struct EslMessage {
    /// Message type
    pub message_type: MessageType,
    /// Message headers
    pub headers: IndexMap<String, String>,
    /// Message body (optional)
    pub body: Option<String>,
}

impl EslMessage {
    /// Create new message
    pub fn new(
        message_type: MessageType,
        headers: IndexMap<String, String>,
        body: Option<String>,
    ) -> Self {
        Self {
            message_type,
            headers,
            body,
        }
    }

    /// Convert to EslResponse
    pub fn into_response(self) -> EslResponse {
        EslResponse::new(self.headers, self.body)
    }
}

/// Parser state for handling incomplete messages
#[derive(Debug)]
enum ParseState {
    WaitingForHeaders,
    WaitingForBody {
        message_type: MessageType,
        headers: IndexMap<String, String>,
        body_length: usize,
    },
}

/// ESL protocol parser
pub struct EslParser {
    buffer: EslBuffer,
    state: ParseState,
}

impl EslParser {
    /// Create new parser
    pub fn new() -> Self {
        Self {
            buffer: EslBuffer::new(),
            state: ParseState::WaitingForHeaders,
        }
    }

    /// Unconsumed bytes remaining in the parser buffer.
    #[cfg(unix)]
    pub fn remaining_bytes(&self) -> &[u8] {
        self.buffer
            .data()
    }

    /// Returns `true` if the parser is between messages (not mid-body).
    #[cfg(unix)]
    pub fn is_waiting_for_headers(&self) -> bool {
        matches!(self.state, ParseState::WaitingForHeaders)
    }

    /// Add data to the parser buffer
    pub fn add_data(&mut self, data: &[u8]) -> EslResult<()> {
        self.buffer
            .extend_from_slice(data);
        self.buffer
            .check_size_limits()?;
        Ok(())
    }

    /// Try to parse a complete message from the buffer
    pub fn parse_message(&mut self) -> EslResult<Option<EslMessage>> {
        match &self.state {
            ParseState::WaitingForHeaders => {
                // Check if we have complete headers
                let terminator = HEADER_TERMINATOR.as_bytes();

                if let Some(headers_data) = self
                    .buffer
                    .extract_until_pattern(terminator)
                {
                    // Compact buffer to free consumed header data
                    self.buffer
                        .compact();

                    // Parse headers
                    let headers_str = String::from_utf8(headers_data)
                        .map_err(|_| EslError::protocol_error("Invalid UTF-8 in headers"))?;

                    let headers = self.parse_headers(&headers_str)?;

                    // Every ESL message must have Content-Type. Missing means
                    // protocol desync (e.g. from a corrupted Content-Length).
                    let content_type = headers
                        .get(HEADER_CONTENT_TYPE)
                        .ok_or_else(|| {
                            EslError::protocol_error(
                                "Missing Content-Type header — likely protocol desync",
                            )
                        })?;
                    let message_type = MessageType::from_content_type(content_type);

                    // Check if we need a body
                    if let Some(length_str) = headers.get(HEADER_CONTENT_LENGTH) {
                        let length: usize = length_str
                            .trim()
                            .parse()
                            .map_err(|_| EslError::InvalidHeader {
                                header: format!("Content-Length: {}", length_str),
                            })?;

                        // Validate message size to prevent protocol errors or memory exhaustion
                        if length > MAX_MESSAGE_SIZE {
                            return Err(EslError::protocol_error(format!(
                                "Message too large: Content-Length {} exceeds limit {}. Protocol error or corrupted data.",
                                length, MAX_MESSAGE_SIZE
                            )));
                        }

                        if length > 0 {
                            // Transition to waiting for body
                            self.state = ParseState::WaitingForBody {
                                message_type,
                                headers,
                                body_length: length,
                            };
                            // Try to parse body immediately
                            self.parse_message()
                        } else {
                            // No body needed, complete message
                            let message = EslMessage::new(message_type, headers, None);
                            self.state = ParseState::WaitingForHeaders;
                            Ok(Some(message))
                        }
                    } else {
                        // No Content-Length header, complete message without body
                        let message = EslMessage::new(message_type, headers, None);
                        self.state = ParseState::WaitingForHeaders;
                        Ok(Some(message))
                    }
                } else {
                    // No complete headers yet
                    Ok(None)
                }
            }
            ParseState::WaitingForBody {
                message_type,
                headers,
                body_length,
            } => {
                if let Some(body_data) = self
                    .buffer
                    .extract_bytes(*body_length)
                {
                    // Compact buffer to free consumed body data
                    self.buffer
                        .compact();

                    let body_str = String::from_utf8(body_data)
                        .map_err(|_| EslError::protocol_error("Invalid UTF-8 in body"))?;

                    let message =
                        EslMessage::new(message_type.clone(), headers.clone(), Some(body_str));
                    self.state = ParseState::WaitingForHeaders;
                    Ok(Some(message))
                } else {
                    // Not enough body data yet
                    Ok(None)
                }
            }
        }
    }

    /// Parse headers from string
    fn parse_headers(&self, headers_str: &str) -> EslResult<IndexMap<String, String>> {
        let mut headers = IndexMap::new();

        for line in headers_str.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(colon_pos) = line.find(':') {
                let key = normalize_header_key(line[..colon_pos].trim());
                let raw_value = line[colon_pos + 1..].trim();
                let value = percent_decode_str(raw_value)
                    .decode_utf8()
                    .map(|s| s.into_owned())
                    .map_err(|e| {
                        EslError::protocol_error(format!(
                            "invalid UTF-8 in header '{}': {}",
                            key, e
                        ))
                    })?;
                headers.insert(key, value);
            } else {
                return Err(EslError::InvalidHeader {
                    header: line.to_string(),
                });
            }
        }

        Ok(headers)
    }

    /// Parse event from message, handling different formats.
    ///
    /// log/data messages use single-level framing (metadata in outer envelope,
    /// raw log text as body) unlike normal events which use two-level framing.
    pub fn parse_event(&self, message: EslMessage, format: EventFormat) -> EslResult<EslEvent> {
        if message
            .headers
            .get(HEADER_CONTENT_TYPE)
            .map(|s| s.as_str())
            == Some(CONTENT_TYPE_LOG_DATA)
        {
            return Self::parse_log_event(message);
        }

        let event = match format {
            EventFormat::Plain => self.parse_plain_event(message),
            EventFormat::Json => self.parse_json_event(message),
            EventFormat::Xml => self.parse_xml_event(message),
            _ => {
                return Err(EslError::ProtocolError {
                    message: format!("unsupported event format: {format}"),
                })
            }
        }?;

        Ok(event)
    }

    /// Parse log/data message.
    ///
    /// FreeSWITCH log/data wire format uses single-level framing, unlike
    /// normal events. Log metadata (Log-Level, Log-File, etc.) lives in the
    /// outer envelope headers and the body is raw log text.
    fn parse_log_event(message: EslMessage) -> EslResult<EslEvent> {
        let mut event = EslEvent::new();
        for (key, value) in &message.headers {
            event.set_header(key.clone(), value.clone());
        }
        if let Some(body) = message.body {
            event.set_body(body);
        }
        event.set_event_type(Some(EslEventType::Log));
        Ok(event)
    }

    /// Parse plain text event
    ///
    /// FreeSWITCH text/event-plain wire format uses a two-part structure:
    /// - Outer envelope: Content-Length + Content-Type headers
    /// - Body: URL-encoded key: value lines (the actual event headers)
    ///
    /// If the event body itself contains a Content-Length, there's an inner
    /// body after the event headers.
    fn parse_plain_event(&self, message: EslMessage) -> EslResult<EslEvent> {
        if message.message_type != MessageType::Event {
            return Err(EslError::protocol_error("Not an event message"));
        }

        let body = message
            .body
            .as_deref()
            .ok_or_else(|| EslError::protocol_error("Plain event missing body"))?;

        let mut event = EslEvent::new();

        // Split event body into headers and optional inner body.
        // Event headers are terminated by \n\n; anything after is the inner body.
        let (header_section, inner_body) = if let Some(pos) = body.find("\n\n") {
            (&body[..pos], Some(&body[pos + 2..]))
        } else {
            (body, None)
        };

        // Parse event headers from the body, percent-decoding values
        for line in header_section.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(colon_pos) = line.find(':') {
                let key = normalize_header_key(line[..colon_pos].trim());
                let raw_value = line[colon_pos + 1..].trim();
                let value = percent_decode_str(raw_value)
                    .decode_utf8()
                    .map(|s| s.into_owned())
                    .map_err(|e| {
                        EslError::protocol_error(format!(
                            "invalid UTF-8 in event header '{}': {}",
                            key, e
                        ))
                    })?;
                event.set_header(key, value);
            } else {
                return Err(EslError::InvalidHeader {
                    header: line.to_string(),
                });
            }
        }

        // If the event headers contain their own Content-Length, the inner body
        // is that many bytes after the header section
        if let Some(ib) = inner_body {
            if !ib.is_empty() {
                event.set_body(ib.to_string());
            }
        }

        if let Some(event_name) = event
            .header(EventHeader::EventName)
            .map(|s| s.to_string())
        {
            event.set_event_type(EslEventType::parse_event_type(&event_name));
        }

        Ok(event)
    }

    /// Parse JSON event
    fn parse_json_event(&self, message: EslMessage) -> EslResult<EslEvent> {
        let body = message
            .body
            .ok_or_else(|| EslError::protocol_error("JSON event missing body"))?;

        // Parse JSON body
        let json_value: serde_json::Value = serde_json::from_str(&body)?;

        let mut event = EslEvent::new();

        if let Some(obj) = json_value.as_object() {
            for (key, value) in obj {
                // FreeSWITCH puts the event body under a "_body" key in JSON events
                if key == "_body" {
                    let body_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => value.to_string(),
                    };
                    event.set_body(body_str);
                    continue;
                }
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                event.set_header(key.clone(), value_str);
            }
        }

        if let Some(event_name) = event
            .header(EventHeader::EventName)
            .map(|s| s.to_string())
        {
            event.set_event_type(EslEventType::parse_event_type(&event_name));
        }

        Ok(event)
    }

    /// Parse XML event using quick_xml.
    ///
    /// FreeSWITCH XML event format:
    /// ```xml
    /// <event>
    ///   <headers>
    ///     <Event-Name>HEARTBEAT</Event-Name>
    ///     <Core-UUID>abc-123</Core-UUID>
    ///   </headers>
    ///   <body>...</body>
    /// </event>
    /// ```
    fn parse_xml_event(&self, message: EslMessage) -> EslResult<EslEvent> {
        use quick_xml::events::Event as XmlEvent;
        use quick_xml::Reader;

        let body = message
            .body
            .ok_or_else(|| EslError::protocol_error("XML event missing body"))?;

        let mut reader = Reader::from_str(&body);
        let mut event = EslEvent::new();
        let mut in_headers = false;
        let mut current_tag: Option<String> = None;
        let mut in_body = false;
        // Accumulator for text content: quick-xml 0.39 splits text around
        // entity references (e.g. "Smith &amp; Jones" becomes Text, GeneralRef,
        // Text), so we must accumulate fragments and flush on End tags.
        let mut text_buf = String::new();

        loop {
            match reader.read_event() {
                Ok(XmlEvent::Start(ref e)) => {
                    let tag = String::from_utf8_lossy(
                        e.name()
                            .as_ref(),
                    )
                    .to_string();
                    match tag.as_str() {
                        "headers" => in_headers = true,
                        "body" => in_body = true,
                        _ if in_headers => {
                            text_buf.clear();
                            current_tag = Some(tag);
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::End(ref e)) => {
                    let tag = String::from_utf8_lossy(
                        e.name()
                            .as_ref(),
                    )
                    .to_string();
                    match tag.as_str() {
                        "headers" => in_headers = false,
                        "body" => {
                            if !text_buf.is_empty() {
                                event.set_body(std::mem::take(&mut text_buf));
                            }
                            in_body = false;
                        }
                        _ if in_headers => {
                            if let Some(ref tag) = current_tag {
                                if !text_buf.is_empty() {
                                    event.set_header(tag.clone(), std::mem::take(&mut text_buf));
                                }
                            }
                            current_tag = None;
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::Text(ref e)) => {
                    let decoded = e
                        .decode()
                        .map_err(quick_xml::Error::from)?;
                    if in_body || current_tag.is_some() {
                        text_buf.push_str(&decoded);
                    }
                }
                Ok(XmlEvent::GeneralRef(ref e)) => {
                    if in_body || current_tag.is_some() {
                        let resolved = Self::resolve_entity(e)?;
                        text_buf.push_str(&resolved);
                    }
                }
                Ok(XmlEvent::Eof) => break,
                Err(e) => return Err(e.into()),
                _ => {}
            }
        }

        if let Some(event_name) = event
            .header(EventHeader::EventName)
            .map(|s| s.to_string())
        {
            event.set_event_type(EslEventType::parse_event_type(&event_name));
        }

        Ok(event)
    }

    /// Resolve an XML entity reference (`&name;` or `&#num;`) to its string value.
    fn resolve_entity(entity: &quick_xml::events::BytesRef<'_>) -> EslResult<String> {
        if let Some(ch) = entity.resolve_char_ref()? {
            return Ok(ch.to_string());
        }
        let name = entity
            .decode()
            .map_err(quick_xml::Error::from)?;
        match quick_xml::escape::resolve_xml_entity(&name) {
            Some(s) => Ok(s.to_string()),
            None => Err(EslError::protocol_error(format!(
                "unknown XML entity: &{};",
                name
            ))),
        }
    }
}

impl Default for EslParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lookup::HeaderLookup;

    #[test]
    fn test_parse_headers() {
        let parser = EslParser::new();
        let headers_str = "Content-Type: auth/request\r\nContent-Length: 0";
        let headers = parser
            .parse_headers(headers_str)
            .unwrap();

        assert_eq!(
            headers
                .get("Content-Type")
                .map(|s| s.as_str()),
            Some("auth/request")
        );
        assert_eq!(
            headers
                .get("Content-Length")
                .map(|s| s.as_str()),
            Some("0")
        );
    }

    #[test]
    fn parsed_headers_preserve_insertion_order() {
        let parser = EslParser::new();
        let headers_str = "Alpha: 1\r\nBravo: 2\r\nCharlie: 3\r\nDelta: 4";
        let headers = parser
            .parse_headers(headers_str)
            .unwrap();
        let keys: Vec<&str> = headers
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(keys, vec!["Alpha", "Bravo", "Charlie", "Delta"]);
    }

    #[test]
    fn test_parse_auth_request() {
        let mut parser = EslParser::new();
        let data = b"Content-Type: auth/request\n\n";

        parser
            .add_data(data)
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();

        assert_eq!(message.message_type, MessageType::AuthRequest);
        assert!(message
            .body
            .is_none());
    }

    #[test]
    fn test_parse_api_response() {
        let mut parser = EslParser::new();
        let data = b"Content-Type: api/response\nContent-Length: 2\n\nOK";

        parser
            .add_data(data)
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();

        assert_eq!(message.message_type, MessageType::ApiResponse);
        assert_eq!(message.body, Some("OK".to_string()));
    }

    #[test]
    fn test_parse_event_plain() {
        let mut parser = EslParser::new();
        // Correct two-part wire format: outer envelope + body with event headers
        let body = "Event-Name: CHANNEL_ANSWER\nUnique-ID: test-uuid\n\n";
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-plain\n\n",
            body.len()
        );
        let data = format!("{}{}", envelope, body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Plain)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::ChannelAnswer));
        assert_eq!(event.unique_id(), Some("test-uuid"));
    }

    #[test]
    fn test_parse_event_plain_percent_decoding() {
        let mut parser = EslParser::new();
        let body = "Event-Name: HEARTBEAT\nUp-Time: 0%20years%2C%200%20days\nEvent-Info: System%20Ready\n\n";
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-plain\n\n",
            body.len()
        );
        let data = format!("{}{}", envelope, body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Plain)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::Heartbeat));
        assert_eq!(event.header_str("Up-Time"), Some("0 years, 0 days"));
        assert_eq!(event.header_str("Event-Info"), Some("System Ready"));
    }

    #[test]
    fn test_parse_event_plain_with_inner_body() {
        let mut parser = EslParser::new();
        // Event with inner body (e.g., BACKGROUND_JOB result)
        let inner_body = "+OK Status\n";
        let event_headers = format!(
            "Event-Name: BACKGROUND_JOB\nJob-UUID: abc-123\nContent-Length: {}\n",
            inner_body.len()
        );
        let body = format!("{}\n{}", event_headers, inner_body);
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-plain\n\n",
            body.len()
        );
        let data = format!("{}{}", envelope, body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Plain)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::BackgroundJob));
        assert_eq!(event.header(EventHeader::JobUuid), Some("abc-123"));
        assert_eq!(event.body(), Some("+OK Status\n"));
    }

    /// log/data uses single-level framing: metadata in outer envelope,
    /// raw log text as body. This matches mod_event_socket.c's output.
    #[test]
    fn test_parse_log_data_event() {
        let mut parser = EslParser::new();
        let log_text = "2024-01-01 00:00:00.000000 [INFO] mod_sofia.c:1234 Registration ok\n";
        let envelope = format!(
            "Content-Type: log/data\nContent-Length: {}\nLog-Level: 6\nText-Channel: 0\nLog-File: mod_sofia.c\nLog-Func: sofia_reg_handle\nLog-Line: 1234\nUser-Data: \n\n{}",
            log_text.len(),
            log_text,
        );

        parser
            .add_data(envelope.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();

        assert_eq!(message.message_type, MessageType::Event);

        let event = parser
            .parse_event(message, EventFormat::Plain)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::Log));
        assert_eq!(event.header(EventHeader::LogLevel), Some("6"));
        assert_eq!(event.header_str("Content-Type"), Some("log/data"));
        assert_eq!(event.header_str("Log-File"), Some("mod_sofia.c"));
        assert_eq!(event.header_str("Log-Func"), Some("sofia_reg_handle"));
        assert_eq!(event.header_str("Log-Line"), Some("1234"));
        assert_eq!(event.body(), Some(log_text));
    }

    #[test]
    fn test_incomplete_message() {
        let mut parser = EslParser::new();
        let data = b"Content-Type: api/response\nContent-Length: 10\n\ntest"; // Only 4 bytes instead of 10

        parser
            .add_data(data)
            .unwrap();
        let result = parser
            .parse_message()
            .unwrap();

        assert!(result.is_none()); // Should return None for incomplete message
    }

    #[test]
    fn test_notify_in_event_with_pl_data() {
        let mut parser = EslParser::new();
        // NOTIFY_IN event with percent-encoded pl_data containing JSON
        let json_payload = r#"{"Invite":"INVITE urn:service:sos SIP/2.0","InviteTimestamp":"2025-01-15T12:00:00Z"}"#;
        let encoded_payload =
            percent_encoding::utf8_percent_encode(json_payload, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        let body = format!(
            "Event-Name: NOTIFY_IN\nevent: emergency-AbandonedCall\npl_data: {}\nsip_content_type: application%2Fjson\ngateway_name: ng911-bcf\n\n",
            encoded_payload
        );
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-plain\n\n",
            body.len()
        );
        let data = format!("{}{}", envelope, body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Plain)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::NotifyIn));
        assert_eq!(event.header_str("event"), Some("emergency-AbandonedCall"));
        // pl_data must be percent-decoded back to raw JSON
        assert_eq!(event.header_str("pl_data"), Some(json_payload));
        assert_eq!(
            event.header_str("sip_content_type"),
            Some("application/json")
        );
        assert_eq!(event.header_str("gateway_name"), Some("ng911-bcf"));
    }

    #[test]
    fn test_parse_event_xml_heartbeat() {
        let mut parser = EslParser::new();
        let xml_body = "\
<event>\n\
  <headers>\n\
    <Event-Name>HEARTBEAT</Event-Name>\n\
    <Core-UUID>abc-123</Core-UUID>\n\
    <Up-Time>0 years, 1 day</Up-Time>\n\
  </headers>\n\
</event>";
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-xml\n\n",
            xml_body.len()
        );
        let data = format!("{}{}", envelope, xml_body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Xml)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::Heartbeat));
        assert_eq!(event.header(EventHeader::CoreUuid), Some("abc-123"));
        assert_eq!(event.header_str("Up-Time"), Some("0 years, 1 day"));
    }

    #[test]
    fn test_parse_event_xml_with_body() {
        let mut parser = EslParser::new();
        let xml_body = "\
<event>\n\
  <headers>\n\
    <Event-Name>BACKGROUND_JOB</Event-Name>\n\
    <Job-UUID>def-456</Job-UUID>\n\
  </headers>\n\
  <body>+OK result data</body>\n\
</event>";
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-xml\n\n",
            xml_body.len()
        );
        let data = format!("{}{}", envelope, xml_body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Xml)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::BackgroundJob));
        assert_eq!(event.header(EventHeader::JobUuid), Some("def-456"));
        assert_eq!(event.body(), Some("+OK result data"));
    }

    #[test]
    fn test_crlf_header_terminator_not_matched() {
        // ESL uses \n\n, not \r\n\r\n. If something injects \r\n line endings,
        // the parser must not hang — but it won't find the terminator either.
        // This documents the current behavior: \r\n\r\n is NOT recognized as
        // a header terminator, so the message stays incomplete.
        let mut parser = EslParser::new();
        let data = b"Content-Type: auth/request\r\n\r\n";

        parser
            .add_data(data)
            .unwrap();
        let result = parser
            .parse_message()
            .unwrap();
        assert!(
            result.is_none(),
            "\\r\\n\\r\\n should not match \\n\\n terminator"
        );
    }

    #[test]
    fn test_crlf_in_header_values_parsed_correctly() {
        // If \r\n appears within a \n\n-framed message, parse_headers()
        // uses .lines() which strips \r, so header values stay clean.
        let mut parser = EslParser::new();
        let data = b"Content-Type: auth/request\r\nSome-Header: some-value\n\n";

        parser
            .add_data(data)
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        assert_eq!(message.message_type, MessageType::AuthRequest);
        assert_eq!(
            message
                .headers
                .get("Some-Header")
                .map(|s| s.as_str()),
            Some("some-value")
        );
    }

    #[test]
    fn test_oversized_content_length_rejected() {
        let mut parser = EslParser::new();
        let data = format!(
            "Content-Type: api/response\nContent-Length: {}\n\n",
            MAX_MESSAGE_SIZE + 1
        );

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let result = parser.parse_message();
        assert!(
            result.is_err(),
            "Content-Length exceeding MAX_MESSAGE_SIZE must be rejected"
        );
    }

    #[test]
    fn test_undersized_content_length_corrupts_next_message() {
        // Content-Length: 2 but body is "Hello" (5 bytes). The parser trusts
        // Content-Length and reads only 2 bytes, leaving "llo" in the buffer.
        // The next parse attempt sees "llo" as the start of a new message,
        // which won't have a valid header terminator — so it returns None.
        let mut parser = EslParser::new();
        let data = b"Content-Type: api/response\nContent-Length: 2\n\nHello";

        parser
            .add_data(data)
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        assert_eq!(message.message_type, MessageType::ApiResponse);
        assert_eq!(message.body, Some("He".to_string()));

        // Leftover "llo" is now junk in the buffer — next parse finds nothing
        let next = parser
            .parse_message()
            .unwrap();
        assert!(
            next.is_none(),
            "Leftover bytes should not form a valid message"
        );
    }

    #[test]
    fn test_undersized_content_length_followed_by_valid_message() {
        // Same scenario but a valid second message follows the junk.
        // The leftover bytes merge with the next message's headers,
        // making recovery impossible without reconnecting.
        let mut parser = EslParser::new();
        let msg1 = b"Content-Type: api/response\nContent-Length: 2\n\nHello";
        let msg2 = b"Content-Type: auth/request\n\n";

        parser
            .add_data(msg1)
            .unwrap();
        let first = parser
            .parse_message()
            .unwrap()
            .unwrap();
        assert_eq!(first.body, Some("He".to_string()));

        parser
            .add_data(msg2)
            .unwrap();
        let second = parser.parse_message();
        // "llo" + msg2 bytes = "lloContent-Type: auth/request\n\n"
        // The parser finds \n\n and parses "lloContent-Type: auth/request"
        // as key="lloContent-Type" value="auth/request". No real Content-Type
        // header exists, so the parser returns a protocol error — signaling
        // the caller to disconnect.
        assert!(
            second.is_err(),
            "Desync must be detected as a protocol error"
        );
    }

    #[test]
    fn test_non_numeric_content_length_rejected() {
        let mut parser = EslParser::new();
        let data = b"Content-Type: api/response\nContent-Length: abc\n\n";

        parser
            .add_data(data)
            .unwrap();
        let result = parser.parse_message();
        assert!(
            result.is_err(),
            "Non-numeric Content-Length must be rejected"
        );
    }

    #[test]
    fn test_parse_headers_percent_decodes_values() {
        let parser = EslParser::new();
        let headers = parser
            .parse_headers("Content-Type: command%2Freply\nReply-Text: %2BOK")
            .unwrap();

        assert_eq!(
            headers
                .get("Content-Type")
                .map(|s| s.as_str()),
            Some("command/reply")
        );
        assert_eq!(
            headers
                .get("Reply-Text")
                .map(|s| s.as_str()),
            Some("+OK")
        );
    }

    #[test]
    fn test_parse_headers_noop_for_plain_values() {
        let parser = EslParser::new();
        let headers = parser
            .parse_headers("Content-Type: command/reply\nReply-Text: +OK")
            .unwrap();

        assert_eq!(
            headers
                .get("Content-Type")
                .map(|s| s.as_str()),
            Some("command/reply")
        );
        assert_eq!(
            headers
                .get("Reply-Text")
                .map(|s| s.as_str()),
            Some("+OK")
        );
    }

    #[test]
    fn test_parse_headers_invalid_percent_sequence() {
        let parser = EslParser::new();
        let headers = parser
            .parse_headers("X-Bad: %ZZinvalid\nX-Good: clean")
            .unwrap();

        assert_eq!(
            headers
                .get("X-Bad")
                .map(|s| s.as_str()),
            Some("%ZZinvalid"),
            "Invalid percent sequence passes through as-is (still valid UTF-8)"
        );
        assert_eq!(
            headers
                .get("X-Good")
                .map(|s| s.as_str()),
            Some("clean")
        );
    }

    #[test]
    fn test_parse_headers_invalid_utf8_is_error() {
        let parser = EslParser::new();
        // %FF decodes to byte 0xFF which is invalid UTF-8
        let result = parser.parse_headers("X-Bad: %FF");
        assert!(
            result.is_err(),
            "invalid UTF-8 after percent-decode must be an error"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid UTF-8"),
            "error should mention invalid UTF-8: {}",
            err
        );
    }

    #[test]
    fn test_parse_plain_event_invalid_utf8_is_error() {
        let parser = EslParser::new();
        let body = "Event-Name: HEARTBEAT\nX-Bad: %FF\n\n";
        let msg = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-plain".to_string());
                h
            },
            Some(body.to_string()),
        );
        let result = parser.parse_event(msg, EventFormat::Plain);
        assert!(
            result.is_err(),
            "invalid UTF-8 in event header must be an error"
        );
    }

    #[test]
    fn test_parse_connect_response() {
        use percent_encoding::{percent_encode, NON_ALPHANUMERIC};

        let mut parser = EslParser::new();

        // Simulate FreeSWITCH's connect response: switch_event_serialize()
        // encodes ALL values, sent as a flat blob (no outer envelope wrapper).
        let headers = [
            ("Content-Type", "command/reply"),
            ("Reply-Text", "+OK"),
            ("Socket-Mode", "async"),
            ("Control", "full"),
            ("Event-Name", "CHANNEL_DATA"),
            ("Channel-Name", "sofia/internal/1000@example.com"),
            ("Unique-ID", "abcd-1234"),
            ("Caller-Caller-ID-Name", "Test User"),
        ];

        let mut data = String::new();
        for (key, value) in &headers {
            data.push_str(&format!(
                "{}: {}\n",
                key,
                percent_encode(value.as_bytes(), NON_ALPHANUMERIC)
            ));
        }
        data.push('\n');

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();

        assert_eq!(message.message_type, MessageType::CommandReply);
        assert_eq!(
            message
                .headers
                .get("Channel-Name")
                .map(|s| s.as_str()),
            Some("sofia/internal/1000@example.com")
        );
        assert_eq!(
            message
                .headers
                .get("Caller-Caller-ID-Name")
                .map(|s| s.as_str()),
            Some("Test User")
        );
        assert_eq!(
            message
                .headers
                .get("Socket-Mode")
                .map(|s| s.as_str()),
            Some("async")
        );
        assert_eq!(
            message
                .headers
                .get("Control")
                .map(|s| s.as_str()),
            Some("full")
        );

        let response = message.into_response();
        assert!(response.is_success());
        assert_eq!(response.reply_text(), Some("+OK"));
    }

    #[test]
    fn test_parse_json_event_body_key() {
        let parser = EslParser::new();
        let json = r#"{"Event-Name":"BACKGROUND_JOB","Job-UUID":"abc-123","_body":"+OK result"}"#;
        let msg = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-json".to_string());
                h
            },
            Some(json.to_string()),
        );
        let event = parser
            .parse_event(msg, EventFormat::Json)
            .unwrap();
        assert_eq!(event.event_type(), Some(EslEventType::BackgroundJob));
        assert_eq!(event.body(), Some("+OK result"));
        assert!(
            event
                .header_str("_body")
                .is_none(),
            "_body must be mapped to event body, not stored as a header"
        );
    }

    // --- T2: JSON event format end-to-end through parser pipeline ---

    #[test]
    fn test_json_event_end_to_end() {
        let mut parser = EslParser::new();
        let json_body = r#"{"Event-Name":"CHANNEL_CREATE","Unique-ID":"test-uuid-123","Channel-Name":"sofia/internal/1000@example.com","variable_sip_call_id":"call-456"}"#;
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-json\n\n",
            json_body.len()
        );
        let data = format!("{}{}", envelope, json_body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        assert_eq!(message.message_type, MessageType::Event);

        let event = parser
            .parse_event(message, EventFormat::Json)
            .unwrap();
        assert_eq!(event.event_type(), Some(EslEventType::ChannelCreate));
        assert_eq!(event.unique_id(), Some("test-uuid-123"));
        assert_eq!(
            event.header_str("Channel-Name"),
            Some("sofia/internal/1000@example.com")
        );
        assert_eq!(event.variable_str("sip_call_id"), Some("call-456"));
    }

    #[test]
    fn test_json_event_with_body_end_to_end() {
        let mut parser = EslParser::new();
        let json_body = r#"{"Event-Name":"BACKGROUND_JOB","Job-UUID":"job-789","_body":"+OK result data\nline 2"}"#;
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-json\n\n",
            json_body.len()
        );
        let data = format!("{}{}", envelope, json_body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Json)
            .unwrap();
        assert_eq!(event.event_type(), Some(EslEventType::BackgroundJob));
        assert!(event
            .body()
            .is_some());
        assert!(event
            .header_str("_body")
            .is_none());
    }

    // --- T5: XML event parsing with &amp; escaped characters ---

    #[test]
    fn test_parse_event_xml_ampersand_escaped() {
        let mut parser = EslParser::new();
        let xml_body = "\
<event>\n\
  <headers>\n\
    <Event-Name>CHANNEL_CREATE</Event-Name>\n\
    <Caller-Caller-ID-Name>Smith &amp; Jones</Caller-Caller-ID-Name>\n\
    <variable_sip_h_Subject>Test &lt;1&gt; &amp; Test &lt;2&gt;</variable_sip_h_Subject>\n\
  </headers>\n\
</event>";
        let envelope = format!(
            "Content-Length: {}\nContent-Type: text/event-xml\n\n",
            xml_body.len()
        );
        let data = format!("{}{}", envelope, xml_body);

        parser
            .add_data(data.as_bytes())
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        let event = parser
            .parse_event(message, EventFormat::Xml)
            .unwrap();

        assert_eq!(event.event_type(), Some(EslEventType::ChannelCreate));
        assert_eq!(
            event.header_str("Caller-Caller-ID-Name"),
            Some("Smith & Jones")
        );
        assert_eq!(
            event.variable_str("sip_h_Subject"),
            Some("Test <1> & Test <2>")
        );
    }

    // --- T6: ParseState::WaitingForBody multi-chunk completion ---

    #[test]
    fn test_waiting_for_body_multi_chunk() {
        let mut parser = EslParser::new();

        // Send headers first (with body length)
        let headers = b"Content-Type: api/response\nContent-Length: 20\n\n";
        parser
            .add_data(headers)
            .unwrap();

        // Parser transitions to WaitingForBody, returns None
        let result = parser
            .parse_message()
            .unwrap();
        assert!(result.is_none(), "should be waiting for body data");

        // Send first chunk (10 of 20 bytes)
        parser
            .add_data(b"0123456789")
            .unwrap();
        let result = parser
            .parse_message()
            .unwrap();
        assert!(result.is_none(), "still waiting for remaining body data");

        // Send remaining 10 bytes
        parser
            .add_data(b"abcdefghij")
            .unwrap();
        let message = parser
            .parse_message()
            .unwrap()
            .unwrap();
        assert_eq!(message.message_type, MessageType::ApiResponse);
        assert_eq!(message.body, Some("0123456789abcdefghij".to_string()));
    }

    #[test]
    fn test_rude_rejection_message_type() {
        let mt = MessageType::from_content_type("text/rude-rejection");
        assert_eq!(mt, MessageType::RudeRejection);
    }

    #[test]
    fn test_to_plain_format_round_trip() {
        use crate::event::{EslEvent, EslEventType, EventFormat};
        use indexmap::IndexMap;

        let mut original = EslEvent::with_type(EslEventType::Heartbeat);
        original.set_header("Event-Name", "HEARTBEAT");
        original.set_header("Core-UUID", "abc-123");
        original.set_header("Up-Time", "0 years, 0 days, 1 hour");
        original.set_header("Event-Info", "System Ready");

        let plain1 = original.to_plain_format();

        let msg1 = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-plain".to_string());
                h
            },
            Some(plain1.clone()),
        );
        let parsed1 = EslParser::new()
            .parse_event(msg1, EventFormat::Plain)
            .unwrap();

        assert_eq!(parsed1.event_type(), original.event_type());
        assert_eq!(parsed1.headers(), original.headers());
        assert_eq!(parsed1.body(), original.body());

        let plain2 = parsed1.to_plain_format();
        let msg2 = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-plain".to_string());
                h
            },
            Some(plain2),
        );
        let parsed2 = EslParser::new()
            .parse_event(msg2, EventFormat::Plain)
            .unwrap();

        assert_eq!(parsed2.event_type(), original.event_type());
        assert_eq!(parsed2.headers(), original.headers());
        assert_eq!(parsed2.body(), original.body());
    }

    #[test]
    fn test_to_plain_format_wire_round_trip() {
        use crate::event::EventFormat;
        use crate::headers::EventHeader;
        use indexmap::IndexMap;

        // Realistic wire payload as FreeSWITCH would send it (percent-encoded
        // values, headers in FS emission order)
        let wire_body = "\
Event-Name: CHANNEL_CREATE\n\
Core-UUID: 2bde6598-0f10-4b90-b70e-d21f4c9e270f\n\
FreeSWITCH-Hostname: fs01%2Eexample%2Ecom\n\
FreeSWITCH-IPv4: 10%2E0%2E0%2E1\n\
Event-Date-Local: 2025-06-15%2010%3A30%3A00\n\
Unique-ID: a1b2c3d4-5678-9abc-def0-123456789abc\n\
Channel-Name: sofia%2Finternal%2F1000%40example.com\n\
Caller-Caller-ID-Name: J%C3%A9r%C3%B4me%20Poulin\n\
Call-Direction: inbound\n\
Channel-State: CS_INIT\n\
\n";

        let msg = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-plain".to_string());
                h
            },
            Some(wire_body.to_string()),
        );
        let event = EslParser::new()
            .parse_event(msg, EventFormat::Plain)
            .unwrap();

        assert_eq!(
            event.header(EventHeader::FreeswitchHostname),
            Some("fs01.example.com")
        );
        assert_eq!(
            event.header(EventHeader::CallerCallerIdName),
            Some("Jérôme Poulin")
        );

        let regenerated = event.to_plain_format();

        // Parse the regenerated output back and compare
        let msg2 = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-plain".to_string());
                h
            },
            Some(regenerated.clone()),
        );
        let reparsed = EslParser::new()
            .parse_event(msg2, EventFormat::Plain)
            .unwrap();
        assert_eq!(event.headers(), reparsed.headers());
        assert_eq!(event.body(), reparsed.body());

        // Verify header order is preserved (wire order, not alphabetical)
        let keys: Vec<&str> = regenerated
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                l.split(':')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(keys[0], "Event-Name");
        assert_eq!(keys[1], "Core-UUID");
        assert_eq!(keys[2], "FreeSWITCH-Hostname");
        assert_eq!(keys[3], "FreeSWITCH-IPv4");
    }

    #[test]
    fn test_to_plain_format_round_trip_with_body() {
        use crate::event::{EslEvent, EslEventType, EventFormat};
        use indexmap::IndexMap;

        let body_text = "+OK Status\nLine 2\n";
        let mut original = EslEvent::with_type(EslEventType::BackgroundJob);
        original.set_header("Event-Name", "BACKGROUND_JOB");
        original.set_header("Job-UUID", "job-789");
        original.set_header(
            "Content-Length".to_string(),
            body_text
                .len()
                .to_string(),
        );
        original.set_body(body_text.to_string());

        let plain = original.to_plain_format();
        let msg = EslMessage::new(
            MessageType::Event,
            {
                let mut h = IndexMap::new();
                h.insert("Content-Type".to_string(), "text/event-plain".to_string());
                h
            },
            Some(plain),
        );
        let parsed = EslParser::new()
            .parse_event(msg, EventFormat::Plain)
            .unwrap();

        assert_eq!(parsed.event_type(), original.event_type());
        assert_eq!(parsed.headers(), original.headers());
        assert_eq!(parsed.body(), original.body());
    }
}
