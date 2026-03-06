//! Connection management for ESL

use std::borrow::Borrow;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio::time::{timeout, Instant};
use tracing::{debug, info, trace, warn};

#[cfg(unix)]
use crate::constants::REEXEC_DRAIN_TIMEOUT_MS;
use crate::{
    command::{EslCommand, EslResponse, ExecuteOptions},
    constants::{
        CONTENT_TYPE_LOG_DATA, DEFAULT_TIMEOUT_MS, HEADER_CONTENT_TYPE, MAX_EVENT_QUEUE_SIZE,
        SOCKET_BUF_SIZE,
    },
    error::{EslError, EslResult},
    event::{EslEvent, EslEventType, EventFormat},
    headers::EventHeader,
    protocol::{EslMessage, EslParser, MessageType},
};

/// Result type for the reexec drain operation (residual bytes or error).
#[cfg(unix)]
type ReexecResult = Result<Vec<u8>, EslError>;

/// Caller-side half of the re-exec channel (stored in `SharedState`).
///
/// Taken by [`EslClient::teardown_for_reexec()`] to signal the reader and
/// receive residual bytes.
#[cfg(unix)]
struct ReexecCaller {
    stop_tx: oneshot::Sender<()>,
    result_rx: oneshot::Receiver<ReexecResult>,
}

/// Reader-side half of the re-exec channel.
///
/// Owned by the reader loop task. The stop receiver fires when teardown is
/// requested; the result sender delivers residual bytes back to the caller.
/// On non-unix platforms this is a zero-size struct (re-exec is unix-only).
struct ReexecReader {
    #[cfg(unix)]
    stop_rx: oneshot::Receiver<()>,
    #[cfg(unix)]
    result_tx: Option<oneshot::Sender<ReexecResult>>,
}

fn event_types_to_string<T: Borrow<EslEventType>>(events: impl IntoIterator<Item = T>) -> String {
    events
        .into_iter()
        .map(|e| {
            e.borrow()
                .as_str()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Connection status for ESL client
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionStatus {
    /// ESL session is active.
    Connected,
    /// ESL session ended.
    Disconnected(DisconnectReason),
}

/// Reason for disconnection
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisconnectReason {
    /// Server sent a `text/disconnect-notice` with `Content-Disposition: disconnect`.
    ///
    /// The notice may include a `Controlled-Session-UUID` header identifying
    /// the session and a body with a human-readable message.
    ServerNotice {
        /// UUID of the controlled session, if present in the notice.
        controlled_session_uuid: Option<String>,
        /// Body text from the disconnect notice (e.g. "Disconnected, goodbye.").
        body: Option<String>,
    },
    /// Liveness timeout exceeded without any inbound traffic
    HeartbeatExpired,
    /// TCP I/O error (io::Error is not Clone, so we store the message)
    IoError(String),
    /// Clean EOF on the TCP connection
    ConnectionClosed,
    /// Client called disconnect()
    ClientRequested,
    /// Client initiated re-exec teardown
    #[cfg(unix)]
    ReexecTeardown,
}

impl std::fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisconnectReason::ServerNotice {
                controlled_session_uuid,
                ..
            } => {
                if let Some(uuid) = controlled_session_uuid {
                    write!(f, "server sent disconnect notice (session {})", uuid)
                } else {
                    write!(f, "server sent disconnect notice")
                }
            }
            DisconnectReason::HeartbeatExpired => write!(f, "liveness timeout expired"),
            DisconnectReason::IoError(msg) => write!(f, "I/O error: {}", msg),
            DisconnectReason::ConnectionClosed => write!(f, "connection closed"),
            DisconnectReason::ClientRequested => write!(f, "client requested disconnect"),
            #[cfg(unix)]
            DisconnectReason::ReexecTeardown => write!(f, "re-exec teardown"),
        }
    }
}

/// Establish a TCP connection with a timeout.
async fn tcp_connect_with_timeout(
    host: &str,
    port: u16,
    connect_timeout: Duration,
) -> EslResult<TcpStream> {
    let timeout_ms = connect_timeout.as_millis() as u64;
    let tcp_result = timeout(connect_timeout, TcpStream::connect((host, port))).await;

    match tcp_result {
        Ok(Ok(s)) => {
            debug!("[CONNECT] TCP connection established");
            Ok(s)
        }
        Ok(Err(e)) => {
            warn!("[CONNECT] TCP connect failed: {}", e);
            Err(EslError::Io(e))
        }
        Err(_) => {
            warn!("[CONNECT] TCP connect timed out after {}ms", timeout_ms);
            Err(EslError::Timeout { timeout_ms })
        }
    }
}

/// Connection mode for ESL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionMode {
    /// Inbound connection - client connects to FreeSWITCH
    Inbound,
    /// Outbound connection - FreeSWITCH connects to client
    Outbound,
}

/// Default command timeout in milliseconds (5 seconds)
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 5000;

/// Shared state between EslClient and the reader task
struct SharedState {
    pending_reply: Mutex<Option<oneshot::Sender<EslMessage>>>,
    /// Connection status sender (shared so disconnect() can set ClientRequested)
    status_tx: watch::Sender<ConnectionStatus>,
    /// Liveness timeout in milliseconds (0 = disabled)
    liveness_timeout_ms: AtomicU64,
    /// Command response timeout in milliseconds
    command_timeout_ms: AtomicU64,
    /// Set when events have been dropped due to a full queue
    event_overflow: AtomicBool,
    /// Total count of dropped events
    dropped_event_count: AtomicU64,
    /// Auth response from inbound connect (None for outbound)
    auth_response: Option<EslResponse>,
    /// Re-exec channel caller half (taken by teardown_for_reexec)
    #[cfg(unix)]
    reexec: Mutex<Option<ReexecCaller>>,
}

/// Options for ESL connection configuration.
///
/// Controls parameters that are fixed at connection time, such as the event
/// queue capacity and connect timeout. Use [`Default::default()`] for standard settings.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EslConnectOptions {
    /// Capacity of the mpsc channel delivering events. Default: 1000.
    pub event_queue_size: usize,
    /// Timeout for TCP connect and each auth handshake read. Default: 2s.
    pub connect_timeout: Duration,
}

impl EslConnectOptions {
    /// Create default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the event queue capacity.
    pub fn with_event_queue_size(mut self, size: usize) -> Self {
        self.event_queue_size = size;
        self
    }

    /// Set the timeout for TCP connect and auth handshake.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
}

impl Default for EslConnectOptions {
    fn default() -> Self {
        Self {
            event_queue_size: MAX_EVENT_QUEUE_SIZE,
            connect_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
        }
    }
}

/// Authentication method for inbound connections.
enum AuthMethod<'a> {
    Password(&'a str),
    User { user: &'a str, password: &'a str },
}

/// ESL client handle (Clone + Send)
///
/// Commands are serialized through the writer mutex. The reader task
/// routes replies to the pending oneshot channel.
#[derive(Clone)]
pub struct EslClient {
    writer: Arc<Mutex<OwnedWriteHalf>>,
    shared: Arc<SharedState>,
    status_rx: watch::Receiver<ConnectionStatus>,
}

impl std::fmt::Debug for EslClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EslClient")
            .field("connected", &self.is_connected())
            .finish()
    }
}

/// Event stream receiver (!Clone)
///
/// Receives events from the background reader task via an mpsc channel.
///
/// Events are delivered as `Result<EslEvent, EslError>`. An `Err(EslError::QueueFull)`
/// indicates that one or more events were dropped because the application fell behind.
/// Use [`EslClient::dropped_event_count`] for the exact count.
pub struct EslEventStream {
    rx: mpsc::Receiver<Result<EslEvent, EslError>>,
    status_rx: watch::Receiver<ConnectionStatus>,
}

impl std::fmt::Debug for EslEventStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EslEventStream")
            .field("connected", &self.is_connected())
            .finish()
    }
}

/// Read a single ESL message from the socket into the parser.
///
/// Used during auth handshake (on unsplit TcpStream) and would be the
/// basis for the reader loop, but the reader loop inlines this logic
/// to handle liveness tracking.
async fn recv_message(
    stream: &mut TcpStream,
    parser: &mut EslParser,
    read_buffer: &mut [u8],
    read_timeout: Duration,
) -> EslResult<EslMessage> {
    let timeout_ms = read_timeout.as_millis() as u64;
    loop {
        if let Some(message) = parser.parse_message()? {
            trace!(
                "[RECV] Parsed message from buffer: {:?}",
                message.message_type
            );
            return Ok(message);
        }

        trace!("[RECV] Buffer needs more data, reading from socket");
        let read_result = timeout(read_timeout, stream.read(read_buffer)).await;

        let bytes_read = match read_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(EslError::Io(e)),
            Err(_) => return Err(EslError::Timeout { timeout_ms }),
        };

        trace!("[RECV] Read {} bytes from socket", bytes_read);
        if bytes_read == 0 {
            return Err(EslError::ConnectionClosed);
        }

        parser.add_data(&read_buffer[..bytes_read])?;
    }
}

/// Perform authentication on the stream, returning the auth response.
///
/// For `userauth`, the response contains `Allowed-Events`, `Allowed-API`,
/// and `Allowed-LOG` headers describing the user's access policy.
async fn authenticate(
    stream: &mut TcpStream,
    parser: &mut EslParser,
    read_buffer: &mut [u8],
    method: AuthMethod<'_>,
    connect_timeout: Duration,
) -> EslResult<EslResponse> {
    debug!("[AUTH] Waiting for auth request from FreeSWITCH");
    let message = recv_message(stream, parser, read_buffer, connect_timeout).await?;

    if message.message_type == MessageType::RudeRejection {
        let reason = message
            .body
            .unwrap_or_else(|| "connection rejected by ACL".to_string());
        return Err(EslError::AccessDenied { reason });
    }

    if message.message_type != MessageType::AuthRequest {
        return Err(EslError::protocol_error("Expected auth request"));
    }

    let auth_cmd = match method {
        AuthMethod::Password(password) => EslCommand::Auth {
            password: password.to_string(),
        },
        AuthMethod::User { user, password } => EslCommand::UserAuth {
            user: user.to_string(),
            password: password.to_string(),
        },
    };

    let command_str = auth_cmd.to_wire_format()?;
    debug!(">> {}", auth_cmd.redact_wire(&command_str));
    stream
        .write_all(command_str.as_bytes())
        .await
        .map_err(EslError::Io)?;

    let response_msg = recv_message(stream, parser, read_buffer, connect_timeout).await?;
    let response = response_msg.into_response();

    if !response.is_success() {
        return Err(match response.reply_text() {
            Some(text) => EslError::auth_failed(text.to_string()),
            None => EslError::protocol_error("auth response missing Reply-Text header"),
        });
    }

    debug!("Authentication successful");
    Ok(response)
}

/// Try to send an event (or error) to the application via try_send.
///
/// If the channel is full, drop the item, set the overflow flag, and
/// increment the dropped counter. Before each dispatch, check the overflow
/// flag and attempt to deliver a QueueFull error notification first.
fn dispatch_event(
    event_tx: &mpsc::Sender<Result<EslEvent, EslError>>,
    shared: &SharedState,
    item: Result<EslEvent, EslError>,
) -> bool {
    if shared
        .event_overflow
        .load(Ordering::Relaxed)
    {
        match event_tx.try_send(Err(EslError::QueueFull)) {
            Ok(()) => {
                shared
                    .event_overflow
                    .store(false, Ordering::Relaxed);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => return false,
            Err(mpsc::error::TrySendError::Full(_)) => {}
        }
    }

    match event_tx.try_send(item) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Closed(_)) => false,
        Err(mpsc::error::TrySendError::Full(_)) => {
            shared
                .event_overflow
                .store(true, Ordering::Relaxed);
            shared
                .dropped_event_count
                .fetch_add(1, Ordering::Relaxed);
            warn!("Event queue full, dropping event");
            true
        }
    }
}

/// Background reader loop
async fn reader_loop(
    reader: OwnedReadHalf,
    parser: EslParser,
    shared: Arc<SharedState>,
    event_tx: mpsc::Sender<Result<EslEvent, EslError>>,
    reexec: ReexecReader,
) {
    let result = std::panic::AssertUnwindSafe(reader_loop_inner(
        reader,
        parser,
        shared.clone(),
        event_tx,
        reexec,
    ));
    if futures_util::FutureExt::catch_unwind(result)
        .await
        .is_err()
    {
        tracing::error!("reader task panicked");
        let _ = shared
            .status_tx
            .send(ConnectionStatus::Disconnected(DisconnectReason::IoError(
                "reader task panicked".to_string(),
            )));
    }
}

async fn reader_loop_inner(
    mut reader: OwnedReadHalf,
    mut parser: EslParser,
    shared: Arc<SharedState>,
    event_tx: mpsc::Sender<Result<EslEvent, EslError>>,
    #[cfg_attr(not(unix), allow(unused_variables, unused_mut))] mut reexec: ReexecReader,
) {
    let mut read_buffer = [0u8; SOCKET_BUF_SIZE];
    let mut last_recv = Instant::now();
    #[cfg(unix)]
    let mut draining = false;

    loop {
        // Try to parse a complete message from buffered data first
        match parser.parse_message() {
            Ok(Some(message)) => {
                match message.message_type {
                    MessageType::Event => {
                        let ct = message
                            .headers
                            .get(HEADER_CONTENT_TYPE)
                            .map(|s| s.as_str());

                        // log/data uses single-level framing handled inside
                        // parse_event; skip the format check for it.
                        let format = if ct == Some(CONTENT_TYPE_LOG_DATA) {
                            EventFormat::Plain
                        } else {
                            match ct.map(EventFormat::from_content_type) {
                                Some(Ok(f)) => f,
                                Some(Err(e)) => {
                                    warn!("Unknown event content type: {}", e);
                                    if !dispatch_event(
                                        &event_tx,
                                        &shared,
                                        Err(EslError::InvalidEventFormat {
                                            format: e
                                                .0
                                                .clone(),
                                        }),
                                    ) {
                                        return;
                                    }
                                    continue;
                                }
                                None => EventFormat::Plain,
                            }
                        };

                        let event_result = parser.parse_event(message, format);
                        if !dispatch_event(&event_tx, &shared, event_result) {
                            debug!("Event channel closed, reader exiting");
                            return;
                        }
                    }
                    MessageType::CommandReply | MessageType::ApiResponse => {
                        let mut pending = shared
                            .pending_reply
                            .lock()
                            .await;
                        if let Some(tx) = pending.take() {
                            let _ = tx.send(message);
                        } else {
                            warn!(
                                "Received {:?} but no pending command",
                                "CommandReply/ApiResponse"
                            );
                        }
                    }
                    MessageType::Disconnect => {
                        let disposition = message
                            .headers
                            .get("Content-Disposition")
                            .map(|s| s.as_str());
                        if disposition == Some("linger") {
                            debug!("Received disconnect notice with linger disposition, ignoring");
                            continue;
                        }
                        let controlled_session_uuid = message
                            .headers
                            .get("Controlled-Session-UUID")
                            .cloned();
                        info!("Received disconnect notice from server");
                        let _ = shared
                            .status_tx
                            .send(ConnectionStatus::Disconnected(
                                DisconnectReason::ServerNotice {
                                    controlled_session_uuid,
                                    body: message.body,
                                },
                            ));
                        return;
                    }
                    MessageType::RudeRejection => {
                        let reason = message
                            .body
                            .unwrap_or_else(|| "connection rejected by ACL".to_string());
                        warn!("Rude rejection from server: {}", reason);
                        let _ = dispatch_event(
                            &event_tx,
                            &shared,
                            Err(EslError::AccessDenied {
                                reason: reason.clone(),
                            }),
                        );
                        let _ = shared
                            .status_tx
                            .send(ConnectionStatus::Disconnected(DisconnectReason::IoError(
                                reason,
                            )));
                        return;
                    }
                    MessageType::AuthRequest | MessageType::Unknown(_) => {
                        debug!("Ignoring unexpected message: {:?}", message.message_type);
                    }
                }
                continue;
            }
            Ok(None) => {
                // Need more data from socket
            }
            Err(e) => {
                warn!("Parser error: {}", e);
                let _ = shared
                    .status_tx
                    .send(ConnectionStatus::Disconnected(DisconnectReason::IoError(
                        e.to_string(),
                    )));
                return;
            }
        }

        // Re-exec drain completion: parser needs more data and is between messages
        #[cfg(unix)]
        if draining && parser.is_waiting_for_headers() {
            let residual = parser
                .remaining_bytes()
                .to_vec();
            debug!("Re-exec drain complete, {} residual bytes", residual.len());
            if let Some(tx) = reexec
                .result_tx
                .take()
            {
                let _ = tx.send(Ok(residual));
            }
            return;
        }

        // Re-exec drain: WaitingForBody, need more socket data to finish the
        // current message before we can stop cleanly.
        #[cfg(unix)]
        if draining {
            let drain_timeout = Duration::from_millis(REEXEC_DRAIN_TIMEOUT_MS);
            match timeout(drain_timeout, reader.read(&mut read_buffer)).await {
                Ok(Ok(0)) => {
                    warn!("Connection closed during re-exec drain");
                    if let Some(tx) = reexec
                        .result_tx
                        .take()
                    {
                        let _ = tx.send(Err(EslError::ReexecFailed {
                            reason: "connection closed during drain".into(),
                        }));
                    }
                    return;
                }
                Ok(Ok(n)) => {
                    if let Err(e) = parser.add_data(&read_buffer[..n]) {
                        if let Some(tx) = reexec
                            .result_tx
                            .take()
                        {
                            let _ = tx.send(Err(e));
                        }
                        return;
                    }
                }
                Ok(Err(e)) => {
                    warn!("Read error during re-exec drain: {}", e);
                    if let Some(tx) = reexec
                        .result_tx
                        .take()
                    {
                        let _ = tx.send(Err(EslError::Io(e)));
                    }
                    return;
                }
                Err(_) => {
                    warn!("Re-exec drain timeout waiting for message body");
                    if let Some(tx) = reexec
                        .result_tx
                        .take()
                    {
                        let _ = tx.send(Err(EslError::ReexecFailed {
                            reason: "drain timeout waiting for message body".into(),
                        }));
                    }
                    return;
                }
            }
            continue;
        }

        // Normal read path with optional reexec stop signal
        #[cfg(unix)]
        let read_result = tokio::select! {
            biased;
            _ = &mut reexec.stop_rx, if !draining => {
                debug!("Re-exec stop signal received, draining parser");
                draining = true;
                continue;
            }
            result = timeout(Duration::from_secs(2), reader.read(&mut read_buffer)) => result,
        };

        #[cfg(not(unix))]
        let read_result = timeout(Duration::from_secs(2), reader.read(&mut read_buffer)).await;

        match read_result {
            Ok(Ok(0)) => {
                info!("Connection closed (EOF)");
                let _ = shared
                    .status_tx
                    .send(ConnectionStatus::Disconnected(
                        DisconnectReason::ConnectionClosed,
                    ));
                return;
            }
            Ok(Ok(n)) => {
                last_recv = Instant::now();
                if let Err(e) = parser.add_data(&read_buffer[..n]) {
                    warn!("Buffer error: {}", e);
                    let _ = shared
                        .status_tx
                        .send(ConnectionStatus::Disconnected(DisconnectReason::IoError(
                            e.to_string(),
                        )));
                    return;
                }
            }
            Ok(Err(e)) => {
                warn!("Read error: {}", e);
                let _ = shared
                    .status_tx
                    .send(ConnectionStatus::Disconnected(DisconnectReason::IoError(
                        e.to_string(),
                    )));
                return;
            }
            Err(_) => {
                // Timeout — check liveness
                let threshold_ms = shared
                    .liveness_timeout_ms
                    .load(Ordering::Relaxed);
                if threshold_ms > 0 {
                    let elapsed = last_recv.elapsed();
                    if elapsed > Duration::from_millis(threshold_ms) {
                        warn!(
                            "Liveness timeout: {}ms without traffic (threshold {}ms)",
                            elapsed.as_millis(),
                            threshold_ms
                        );
                        let _ = shared
                            .status_tx
                            .send(ConnectionStatus::Disconnected(
                                DisconnectReason::HeartbeatExpired,
                            ));
                        return;
                    }
                }
            }
        }
    }
}

impl EslClient {
    /// Connect to FreeSWITCH (inbound mode) with password authentication
    pub async fn connect(
        host: &str,
        port: u16,
        password: &str,
    ) -> EslResult<(Self, EslEventStream)> {
        Self::connect_inner(
            host,
            port,
            AuthMethod::Password(password),
            EslConnectOptions::default(),
        )
        .await
    }

    /// Connect to FreeSWITCH (inbound mode) with password authentication and custom options
    pub async fn connect_with_options(
        host: &str,
        port: u16,
        password: &str,
        options: EslConnectOptions,
    ) -> EslResult<(Self, EslEventStream)> {
        Self::connect_inner(host, port, AuthMethod::Password(password), options).await
    }

    /// Connect with user authentication
    ///
    /// The user must be in the format `user@domain` (e.g., `admin@default`).
    pub async fn connect_with_user(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
    ) -> EslResult<(Self, EslEventStream)> {
        Self::connect_inner(
            host,
            port,
            AuthMethod::User { user, password },
            EslConnectOptions::default(),
        )
        .await
    }

    /// Connect with user authentication and custom options
    ///
    /// The user must be in the format `user@domain` (e.g., `admin@default`).
    pub async fn connect_with_user_and_options(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        options: EslConnectOptions,
    ) -> EslResult<(Self, EslEventStream)> {
        Self::connect_inner(host, port, AuthMethod::User { user, password }, options).await
    }

    async fn connect_inner(
        host: &str,
        port: u16,
        method: AuthMethod<'_>,
        options: EslConnectOptions,
    ) -> EslResult<(Self, EslEventStream)> {
        if let AuthMethod::User { user, .. } = &method {
            if !user.contains('@') {
                return Err(EslError::auth_failed(format!(
                    "Invalid username format '{}': must be user@domain (e.g., admin@default)",
                    user
                )));
            }
        }

        info!("Connecting to FreeSWITCH at {}:{}", host, port);

        let connect_timeout = options.connect_timeout;
        let mut stream = tcp_connect_with_timeout(host, port, connect_timeout).await?;
        let mut parser = EslParser::new();
        let mut read_buffer = [0u8; SOCKET_BUF_SIZE];

        let auth_response = authenticate(
            &mut stream,
            &mut parser,
            &mut read_buffer,
            method,
            connect_timeout,
        )
        .await?;

        info!("Successfully connected and authenticated to FreeSWITCH");
        Ok(Self::split_and_spawn_with_options(
            stream,
            parser,
            options,
            Some(auth_response),
        ))
    }

    /// Accept outbound connection from FreeSWITCH
    pub async fn accept_outbound(listener: &TcpListener) -> EslResult<(Self, EslEventStream)> {
        Self::accept_outbound_with_options(listener, EslConnectOptions::default()).await
    }

    /// Accept outbound connection from FreeSWITCH with custom options
    pub async fn accept_outbound_with_options(
        listener: &TcpListener,
        options: EslConnectOptions,
    ) -> EslResult<(Self, EslEventStream)> {
        info!("Waiting for outbound connection from FreeSWITCH");

        let (stream, addr) = listener
            .accept()
            .await
            .map_err(EslError::Io)?;
        info!("Accepted outbound connection from {}", addr);

        Ok(Self::accept_outbound_stream_with_options(stream, options))
    }

    /// Create an outbound-mode client from an already-accepted `TcpStream`.
    ///
    /// Use this when you need control over the accept step (e.g. for
    /// timeouts, TLS wrapping, or custom accept logic).
    pub fn accept_outbound_stream(stream: TcpStream) -> (Self, EslEventStream) {
        Self::accept_outbound_stream_with_options(stream, EslConnectOptions::default())
    }

    /// Create an outbound-mode client from an already-accepted `TcpStream`
    /// with custom options.
    pub fn accept_outbound_stream_with_options(
        stream: TcpStream,
        options: EslConnectOptions,
    ) -> (Self, EslEventStream) {
        Self::split_and_spawn_with_options(stream, EslParser::new(), options, None)
    }

    /// Create an `EslClient` from an already-authenticated TCP stream.
    ///
    /// Used after re-exec: the previous process already authenticated and
    /// subscribed to events. The `residual_bytes` are any unparsed data
    /// left in the previous parser buffer (from
    /// [`teardown_for_reexec()`](Self::teardown_for_reexec)).
    ///
    /// FreeSWITCH server-side event subscriptions survive across `exec()`.
    /// Events will arrive immediately, so the caller should be ready to
    /// consume them before calling this method.
    pub fn adopt_stream(
        stream: TcpStream,
        residual_bytes: &[u8],
    ) -> EslResult<(Self, EslEventStream)> {
        Self::adopt_stream_with_options(stream, residual_bytes, EslConnectOptions::default())
    }

    /// Create an `EslClient` from an already-authenticated TCP stream
    /// with custom options.
    ///
    /// See [`adopt_stream()`](Self::adopt_stream) for details.
    pub fn adopt_stream_with_options(
        stream: TcpStream,
        residual_bytes: &[u8],
        options: EslConnectOptions,
    ) -> EslResult<(Self, EslEventStream)> {
        let mut parser = EslParser::new();
        if !residual_bytes.is_empty() {
            parser.add_data(residual_bytes)?;
        }
        Ok(Self::split_and_spawn_with_options(
            stream, parser, options, None,
        ))
    }

    fn split_and_spawn_with_options(
        stream: TcpStream,
        parser: EslParser,
        options: EslConnectOptions,
        auth_response: Option<EslResponse>,
    ) -> (Self, EslEventStream) {
        let queue_size = options
            .event_queue_size
            .max(1);

        let (read_half, write_half) = stream.into_split();

        let (status_tx, status_rx) = watch::channel(ConnectionStatus::Connected);
        let status_rx2 = status_tx.subscribe();

        #[cfg(unix)]
        let (stop_tx, stop_rx) = oneshot::channel();
        #[cfg(unix)]
        let (result_tx, result_rx) = oneshot::channel();

        let shared = Arc::new(SharedState {
            pending_reply: Mutex::new(None),
            status_tx,
            liveness_timeout_ms: AtomicU64::new(0),
            command_timeout_ms: AtomicU64::new(DEFAULT_COMMAND_TIMEOUT_MS),
            event_overflow: AtomicBool::new(false),
            dropped_event_count: AtomicU64::new(0),
            auth_response,
            #[cfg(unix)]
            reexec: Mutex::new(Some(ReexecCaller { stop_tx, result_rx })),
        });
        let (event_tx, event_rx) = mpsc::channel(queue_size);

        #[cfg(unix)]
        let reexec_reader = ReexecReader {
            stop_rx,
            result_tx: Some(result_tx),
        };
        #[cfg(not(unix))]
        let reexec_reader = ReexecReader {};

        tokio::spawn(reader_loop(
            read_half,
            parser,
            shared.clone(),
            event_tx,
            reexec_reader,
        ));

        let client = EslClient {
            writer: Arc::new(Mutex::new(write_half)),
            shared,
            status_rx,
        };

        let stream = EslEventStream {
            rx: event_rx,
            status_rx: status_rx2,
        };

        (client, stream)
    }

    /// Send a command and wait for the reply.
    ///
    /// The writer lock is held through the entire send-and-receive cycle to
    /// prevent concurrent commands from overwriting the pending reply slot
    /// (ESL is a sequential request/response protocol).
    pub async fn send_command(&self, command: EslCommand) -> EslResult<EslResponse> {
        if !self.is_connected() {
            return Err(EslError::NotConnected);
        }

        let command_str = command.to_wire_format()?;
        debug!(">> {}", command.redact_wire(&command_str));

        // Lock writer — serializes concurrent commands and holds through reply.
        let mut writer = self
            .writer
            .lock()
            .await;

        // Set up reply channel
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self
                .shared
                .pending_reply
                .lock()
                .await;
            *pending = Some(tx);
        }

        // Write command
        writer
            .write_all(command_str.as_bytes())
            .await
            .map_err(EslError::Io)?;

        // Wait for reply from reader task with command timeout (writer still locked)
        let timeout_ms = self
            .shared
            .command_timeout_ms
            .load(Ordering::Relaxed);
        let message = match timeout(Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(message)) => message,
            Ok(Err(_)) => {
                drop(writer);
                return Err(EslError::ConnectionClosed);
            }
            Err(_) => {
                let mut pending = self
                    .shared
                    .pending_reply
                    .lock()
                    .await;
                pending.take();
                drop(writer);
                return Err(EslError::Timeout { timeout_ms });
            }
        };

        drop(writer);

        let response = message.into_response();
        debug!("Received response: success={}", response.is_success());
        Ok(response)
    }

    /// Send a command and require a successful response, discarding the body.
    async fn send_command_ok(&self, command: EslCommand) -> EslResult<()> {
        self.send_command(command)
            .await?
            .into_result()
            .map(|_| ())
    }

    /// Execute API command synchronously.
    ///
    /// **Warning: this blocks the entire ESL socket.** FreeSWITCH processes
    /// `api` commands inline — no events are delivered and no other commands
    /// can be sent on this connection until the command finishes. For commands
    /// that may take a long time (`originate`, `conference`, bulk operations),
    /// use [`bgapi`](Self::bgapi) instead so events keep flowing.
    ///
    /// Use [`EslResponse::api_result`] to parse the response body, or
    /// [`parse_api_body`](crate::parse_api_body) for `BACKGROUND_JOB` event
    /// bodies.
    ///
    /// ```rust,no_run
    /// # async fn example(client: &freeswitch_esl_tokio::EslClient) -> Result<(), freeswitch_esl_tokio::EslError> {
    /// let resp = client.api("status").await?;
    /// let status = resp.api_result()?;
    /// println!("{}", status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn api(&self, command: &str) -> EslResult<EslResponse> {
        let cmd = EslCommand::Api {
            command: command.to_string(),
        };
        self.send_command(cmd)
            .await
    }

    /// Execute background API command.
    ///
    /// Returns immediately with a `Job-UUID` in the response. The actual result
    /// arrives later as a [`EslEventType::BackgroundJob`] event — subscribe to it
    /// and correlate via [`HeaderLookup::job_uuid()`](crate::HeaderLookup::job_uuid) / [`EslResponse::job_uuid`]:
    ///
    /// ```rust,no_run
    /// # async fn example(client: &freeswitch_esl_tokio::EslClient) -> Result<(), freeswitch_esl_tokio::EslError> {
    /// let resp = client.bgapi("originate user/1000 &park").await?;
    /// let job_uuid = resp.job_uuid().expect("bgapi returns Job-UUID header");
    /// // Match against event.job_uuid() in the event loop
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bgapi(&self, command: &str) -> EslResult<EslResponse> {
        let cmd = EslCommand::BgApi {
            command: command.to_string(),
        };
        self.send_command(cmd)
            .await
    }

    /// Subscribe to events by typed enum variants.
    ///
    /// For `CUSTOM` event subclasses (e.g., `sofia::register`), use
    /// [`subscribe_events_raw`](Self::subscribe_events_raw) instead — this method
    /// sends bare `CUSTOM` which subscribes to **all** custom events:
    ///
    /// ```rust,no_run
    /// # async fn example(client: &freeswitch_esl_tokio::EslClient) -> Result<(), freeswitch_esl_tokio::EslError> {
    /// use freeswitch_esl_tokio::EventFormat;
    /// // Subscribe to specific CUSTOM subclasses:
    /// client.subscribe_events_raw(EventFormat::Plain, "CUSTOM sofia::register sofia::unregister").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn subscribe_events<T: Borrow<EslEventType>>(
        &self,
        format: EventFormat,
        events: impl IntoIterator<Item = T>,
    ) -> EslResult<()> {
        let events: Vec<EslEventType> = events
            .into_iter()
            .map(|e| *e.borrow())
            .collect();
        let events_str = if events.contains(&EslEventType::All) {
            "ALL".to_string()
        } else {
            event_types_to_string(events)
        };

        let cmd = EslCommand::Events {
            format: format.to_string(),
            events: events_str,
        };

        self.send_command_ok(cmd)
            .await?;
        info!("Subscribed to events with format {:?}", format);
        Ok(())
    }

    /// Subscribe to events using raw event name strings.
    ///
    /// Use this for event types not covered by `EslEventType`, or for
    /// forward compatibility with new FreeSWITCH events without a library update.
    ///
    /// ```rust,no_run
    /// # async fn example(client: &freeswitch_esl_tokio::EslClient) -> Result<(), freeswitch_esl_tokio::EslError> {
    /// use freeswitch_esl_tokio::EventFormat;
    /// client.subscribe_events_raw(EventFormat::Plain, "NOTIFY_IN CHANNEL_CREATE").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn subscribe_events_raw(&self, format: EventFormat, events: &str) -> EslResult<()> {
        let cmd = EslCommand::Events {
            format: format.to_string(),
            events: events.to_string(),
        };

        self.send_command_ok(cmd)
            .await?;
        info!(
            "Subscribed to raw events '{}' with format {:?}",
            events, format
        );
        Ok(())
    }

    /// Set event filter using a typed [`EventHeader`].
    pub async fn filter(&self, header: EventHeader, value: &str) -> EslResult<()> {
        self.filter_raw(header.as_str(), value)
            .await
    }

    /// Set event filter using a raw header name string.
    pub async fn filter_raw(&self, header: &str, value: &str) -> EslResult<()> {
        let cmd = EslCommand::Filter {
            header: header.to_string(),
            value: value.to_string(),
        };

        self.send_command_ok(cmd)
            .await?;
        debug!("Set event filter: {} = {}", header, value);
        Ok(())
    }

    /// Execute application on channel.
    pub async fn execute(
        &self,
        app: &str,
        args: Option<&str>,
        uuid: Option<&str>,
    ) -> EslResult<EslResponse> {
        self.execute_with_options(app, args, uuid, ExecuteOptions::default())
            .await
    }

    /// Execute application on channel with custom options.
    ///
    /// See [`ExecuteOptions`] for available flags (`event-lock`, `async`, `loops`).
    pub async fn execute_with_options(
        &self,
        app: &str,
        args: Option<&str>,
        uuid: Option<&str>,
        options: ExecuteOptions,
    ) -> EslResult<EslResponse> {
        let cmd = EslCommand::Execute {
            app: app.to_string(),
            args: args.map(|s| s.to_string()),
            uuid: uuid.map(|s| s.to_string()),
            options,
        };
        self.send_command(cmd)
            .await
    }

    /// Send message to channel
    pub async fn sendmsg(&self, uuid: Option<&str>, event: EslEvent) -> EslResult<EslResponse> {
        let cmd = EslCommand::SendMsg {
            uuid: uuid.map(|s| s.to_string()),
            event,
        };
        self.send_command(cmd)
            .await
    }

    /// Fire an event into FreeSWITCH's event bus.
    ///
    /// Headers and body from the event are sent as-is (not percent-encoded).
    /// If the event has a `unique-id` header, FreeSWITCH also queues it
    /// directly to that session.
    pub async fn sendevent(&self, event: EslEvent) -> EslResult<EslResponse> {
        self.send_command(EslCommand::SendEvent { event })
            .await
    }

    /// Subscribe to session events (outbound mode, no UUID needed).
    ///
    /// Subscribes to all channel-related events for the attached session.
    /// In outbound mode, FreeSWITCH already knows the session UUID.
    pub async fn myevents(&self, format: EventFormat) -> EslResult<()> {
        let cmd = EslCommand::MyEvents {
            format: format.to_string(),
            uuid: None,
        };
        self.send_command_ok(cmd)
            .await
    }

    /// Subscribe to session events for a specific UUID (inbound mode).
    ///
    /// Subscribes to all channel-related events for the given session UUID.
    /// Use this in inbound mode where no session is attached to the socket.
    pub async fn myevents_uuid(&self, uuid: &str, format: EventFormat) -> EslResult<()> {
        let cmd = EslCommand::MyEvents {
            format: format.to_string(),
            uuid: Some(uuid.to_string()),
        };
        self.send_command_ok(cmd)
            .await
    }

    /// Keep the socket open after the channel hangs up (outbound mode).
    ///
    /// Without linger, the socket closes immediately on hangup. With linger,
    /// FreeSWITCH sends a `text/disconnect-notice` with `Content-Disposition: linger`
    /// and keeps the socket open so the client can drain remaining events.
    ///
    /// Pass `None` for indefinite linger, or `Some(Duration)` for a timeout.
    pub async fn linger(&self, timeout: Option<Duration>) -> EslResult<()> {
        self.send_command_ok(EslCommand::Linger { timeout })
            .await
    }

    /// Cancel linger mode (outbound mode).
    ///
    /// Only effective before the channel hangs up. After the disconnect notice
    /// has been sent, it's too late to cancel.
    pub async fn nolinger(&self) -> EslResult<()> {
        self.send_command_ok(EslCommand::NoLinger)
            .await
    }

    /// Resume dialplan execution when the socket disconnects (outbound mode).
    ///
    /// Without resume, the channel is hung up when the socket application exits.
    /// With resume, FreeSWITCH continues dialplan execution from where it left off.
    pub async fn resume(&self) -> EslResult<()> {
        self.send_command_ok(EslCommand::Resume)
            .await
    }

    /// Establish the outbound session by sending `connect` and receiving channel data.
    ///
    /// In outbound mode, this **must** be the first command sent after
    /// [`accept_outbound`](Self::accept_outbound). FreeSWITCH replies with a
    /// `command/reply` containing all channel variables for the session.
    ///
    /// The returned [`EslResponse`] contains the channel data as headers.
    /// Use [`EslResponse::header()`] to read individual channel variables
    /// (e.g., `Caller-Caller-ID-Number`, `Channel-Name`).
    pub async fn connect_session(&self) -> EslResult<EslResponse> {
        self.send_command(EslCommand::Connect)
            .await
    }

    /// Unsubscribe from specific events by typed enum variants.
    ///
    /// The inverse of [`subscribe_events`](Self::subscribe_events). Accepts
    /// multiple event types to unsubscribe from at once.
    pub async fn nixevent<T: Borrow<EslEventType>>(
        &self,
        events: impl IntoIterator<Item = T>,
    ) -> EslResult<()> {
        self.nixevent_raw(&event_types_to_string(events))
            .await
    }

    /// Unsubscribe from events using raw event name strings.
    ///
    /// Use this for event types not covered by `EslEventType`, or for
    /// forward compatibility with new FreeSWITCH events without a library update.
    pub async fn nixevent_raw(&self, events: &str) -> EslResult<()> {
        let cmd = EslCommand::NixEvent {
            events: events.to_string(),
        };
        self.send_command_ok(cmd)
            .await
    }

    /// Unsubscribe from all events.
    ///
    /// Clears all event subscriptions. The server flushes any queued events.
    pub async fn noevents(&self) -> EslResult<()> {
        self.send_command_ok(EslCommand::NoEvents)
            .await
    }

    /// Remove an event filter for a typed [`EventHeader`].
    ///
    /// Without a value, removes all filters for the given header.
    /// With a value, removes only the filter matching that header+value pair.
    pub async fn filter_delete(&self, header: EventHeader, value: Option<&str>) -> EslResult<()> {
        self.filter_delete_raw(header.as_str(), value)
            .await
    }

    /// Remove an event filter using a raw header name string.
    ///
    /// Without a value, removes all filters for the given header.
    /// With a value, removes only the filter matching that header+value pair.
    pub async fn filter_delete_raw(&self, header: &str, value: Option<&str>) -> EslResult<()> {
        let cmd = EslCommand::FilterDelete {
            header: header.to_string(),
            value: value.map(|v| v.to_string()),
        };
        self.send_command_ok(cmd)
            .await
    }

    /// Remove all event filters.
    pub async fn filter_delete_all(&self) -> EslResult<()> {
        self.filter_delete_raw("all", None)
            .await
    }

    /// Redirect session events to the ESL connection (outbound mode).
    ///
    /// When `on` is true, events that would normally be processed internally
    /// by FreeSWITCH are instead sent to the ESL connection.
    pub async fn divert_events(&self, on: bool) -> EslResult<()> {
        self.send_command_ok(EslCommand::DivertEvents { on })
            .await
    }

    /// Read a channel variable (outbound mode).
    ///
    /// **Protocol quirk:** Unlike every other ESL command, `getvar` returns
    /// the raw variable value directly in `Reply-Text` with no `+OK`/`-ERR`
    /// prefix. A non-existent variable returns an empty string (never `-ERR`).
    /// This method reads the raw Reply-Text; do not use `into_result()` on
    /// the response — it would misclassify the bare value as
    /// [`UnexpectedReply`](crate::EslError::UnexpectedReply).
    pub async fn getvar(&self, name: &str) -> EslResult<String> {
        let cmd = EslCommand::GetVar {
            name: name.to_string(),
        };
        let response = self
            .send_command(cmd)
            .await?;
        response
            .reply_text()
            .map(|s| s.to_string())
            .ok_or_else(|| EslError::protocol_error("getvar response missing Reply-Text header"))
    }

    /// Enable FreeSWITCH log forwarding at the given level.
    ///
    /// Log messages stream as events with `Content-Type: log/data`.
    /// Valid levels: `DEBUG`, `INFO`, `NOTICE`, `WARNING`, `ERROR`,
    /// `CRIT`, `ALERT`, `EMERG` (or numeric 0–7).
    pub async fn log(&self, level: &str) -> EslResult<EslResponse> {
        let cmd = EslCommand::Log {
            level: level.to_string(),
        };
        self.send_command(cmd)
            .await
    }

    /// Disable log forwarding.
    pub async fn nolog(&self) -> EslResult<EslResponse> {
        self.send_command(EslCommand::NoLog)
            .await
    }

    /// Send a no-op command (keepalive).
    pub async fn noop(&self) -> EslResult<EslResponse> {
        self.send_command(EslCommand::NoOp)
            .await
    }

    /// Send the `exit` command to gracefully close the ESL session.
    ///
    /// Unlike [`disconnect()`](Self::disconnect) which shuts down the TCP
    /// write half immediately, this sends the ESL `exit` command and waits
    /// for the server's reply before the connection closes.
    pub async fn exit(&self) -> EslResult<EslResponse> {
        self.send_command(EslCommand::Exit)
            .await
    }

    /// Authentication response from the inbound connect handshake.
    ///
    /// For `userauth`, contains `Allowed-Events`, `Allowed-API`, and
    /// `Allowed-LOG` headers describing the user's access policy.
    /// Returns `None` for outbound connections (no auth handshake).
    pub fn auth_response(&self) -> Option<&EslResponse> {
        self.shared
            .auth_response
            .as_ref()
    }

    /// Number of events dropped due to a full event queue.
    pub fn dropped_event_count(&self) -> u64 {
        self.shared
            .dropped_event_count
            .load(Ordering::Relaxed)
    }

    /// Set liveness timeout. Any inbound TCP traffic resets the timer.
    /// Set to zero to disable (default).
    ///
    /// Requires an active subscription to [`EslEventType::Heartbeat`] so
    /// FreeSWITCH sends periodic traffic on idle connections. Without it,
    /// the timer will expire and the connection will be closed.
    pub fn set_liveness_timeout(&self, duration: Duration) {
        self.shared
            .liveness_timeout_ms
            .store(duration.as_millis() as u64, Ordering::Relaxed);
    }

    /// Set command response timeout (default: 5 seconds).
    ///
    /// Applies to `send_command()`, `api()`, `bgapi()`, `subscribe_events()`,
    /// and all other methods that send a command and await a reply.
    ///
    /// If increased for long-running `api()` calls, also increase or disable
    /// the liveness timeout — `api` blocks the socket, starving the liveness timer.
    pub fn set_command_timeout(&self, duration: Duration) {
        self.shared
            .command_timeout_ms
            .store(duration.as_millis() as u64, Ordering::Relaxed);
    }

    /// Whether the connection is alive (not yet disconnected).
    pub fn is_connected(&self) -> bool {
        matches!(
            *self
                .status_rx
                .borrow(),
            ConnectionStatus::Connected
        )
    }

    /// Current connection status snapshot.
    pub fn status(&self) -> ConnectionStatus {
        self.status_rx
            .borrow()
            .clone()
    }

    /// Disconnect from FreeSWITCH by shutting down the write half.
    ///
    /// Sets the connection status to [`DisconnectReason::ClientRequested`]
    /// before closing the socket, so callers can distinguish client-initiated
    /// disconnects from server-initiated ones.
    pub async fn disconnect(&self) -> EslResult<()> {
        info!("Client requested disconnect");
        let _ = self
            .shared
            .status_tx
            .send(ConnectionStatus::Disconnected(
                DisconnectReason::ClientRequested,
            ));
        let mut writer = self
            .writer
            .lock()
            .await;
        writer
            .shutdown()
            .await
            .map_err(EslError::Io)?;
        Ok(())
    }
}

#[cfg(unix)]
impl EslClient {
    /// Gracefully stop the reader loop and return the raw socket fd and any
    /// unparsed bytes remaining in the parser buffer.
    ///
    /// Used for zero-downtime binary upgrades via `exec()`. The caller
    /// serializes application state, clears `CLOEXEC` on the returned fd,
    /// and calls `exec()`. The new process reconstructs the connection with
    /// [`adopt_stream()`](Self::adopt_stream).
    ///
    /// # Preconditions
    ///
    /// - No ESL command may be in-flight (pending reply). Returns
    ///   [`EslError::ReexecFailed`] if a command is pending.
    /// - May only be called once. Returns an error on subsequent calls.
    ///
    /// # After this call
    ///
    /// - The connection status is set to
    ///   [`DisconnectReason::ReexecTeardown`], so all clones see the
    ///   connection as dead.
    /// - The caller **must not drop** the `EslClient` before `exec()` (or
    ///   must [`std::mem::forget`] it) to keep the fd open.
    /// - The caller is responsible for clearing `CLOEXEC` on the fd.
    pub async fn teardown_for_reexec(&self) -> EslResult<(std::os::unix::io::RawFd, Vec<u8>)> {
        use std::os::unix::io::AsRawFd;

        // Reject if a command is in-flight
        {
            let pending = self
                .shared
                .pending_reply
                .lock()
                .await;
            if pending.is_some() {
                return Err(EslError::ReexecFailed {
                    reason: "command still in-flight".into(),
                });
            }
        }

        // Take the reexec channel (one-shot: fails on second call)
        let reexec = {
            let mut guard = self
                .shared
                .reexec
                .lock()
                .await;
            guard
                .take()
                .ok_or_else(|| EslError::ReexecFailed {
                    reason: "teardown already called".into(),
                })?
        };

        // Disable liveness to prevent HeartbeatExpired race
        self.shared
            .liveness_timeout_ms
            .store(0, Ordering::Relaxed);

        let _ = reexec
            .stop_tx
            .send(());

        // Wait for reader to drain and return residual bytes.
        // Extra 1s margin so the reader's own drain timeout fires first
        // with a descriptive error.
        let outer_timeout = Duration::from_millis(REEXEC_DRAIN_TIMEOUT_MS) + Duration::from_secs(1);
        let residual = timeout(outer_timeout, reexec.result_rx)
            .await
            .map_err(|_| EslError::ReexecFailed {
                reason: "timed out waiting for reader to stop".into(),
            })?
            .map_err(|_| EslError::ReexecFailed {
                reason: "reader task exited without sending result".into(),
            })??;

        // Get fd from writer
        let writer = self
            .writer
            .lock()
            .await;
        let fd = writer
            .as_ref()
            .as_raw_fd();

        // Mark connection as dead so other clones see it
        let _ = self
            .shared
            .status_tx
            .send(ConnectionStatus::Disconnected(
                DisconnectReason::ReexecTeardown,
            ));

        info!(
            "Re-exec teardown complete, fd={}, {} residual bytes",
            fd,
            residual.len()
        );
        Ok((fd, residual))
    }
}

impl EslEventStream {
    /// Receive the next event, or None if the channel is closed.
    ///
    /// Returns `Err(EslError::QueueFull)` if events were dropped because the
    /// application was not draining events fast enough. This is a one-time
    /// notification per overflow episode — subsequent calls return real events.
    /// Parse errors from the reader task are also surfaced here.
    pub async fn recv(&mut self) -> Option<Result<EslEvent, EslError>> {
        self.rx
            .recv()
            .await
    }

    /// Whether the connection is alive (not yet disconnected).
    pub fn is_connected(&self) -> bool {
        matches!(
            *self
                .status_rx
                .borrow(),
            ConnectionStatus::Connected
        )
    }

    /// Current connection status snapshot.
    pub fn status(&self) -> ConnectionStatus {
        self.status_rx
            .borrow()
            .clone()
    }
}

impl futures_util::Stream for EslEventStream {
    type Item = Result<EslEvent, EslError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx
            .poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_mode() {
        assert_eq!(ConnectionMode::Inbound, ConnectionMode::Inbound);
        assert_ne!(ConnectionMode::Inbound, ConnectionMode::Outbound);
    }

    #[tokio::test]
    async fn test_connection_status_eq() {
        assert_eq!(ConnectionStatus::Connected, ConnectionStatus::Connected);
        assert_eq!(
            ConnectionStatus::Disconnected(DisconnectReason::ServerNotice {
                controlled_session_uuid: None,
                body: None,
            }),
            ConnectionStatus::Disconnected(DisconnectReason::ServerNotice {
                controlled_session_uuid: None,
                body: None,
            })
        );
        assert_ne!(
            ConnectionStatus::Connected,
            ConnectionStatus::Disconnected(DisconnectReason::ConnectionClosed)
        );
    }
}
