//! Reconnecting ESL client -- production pattern for persistent event listeners.
//!
//! Demonstrates the recommended reconnection loop using the library's error
//! classification. The library never reconnects automatically; instead it
//! classifies errors so the caller can implement the right policy:
//!
//! - **Auth errors** (`AuthenticationFailed`) -- permanent config error, exit
//!   immediately with `EX_CONFIG` (78) so systemd won't restart.
//! - **Connection errors** (`is_connection_error()`) -- transient, reconnect
//!   with exponential backoff.
//! - **Recoverable errors** (`is_recoverable()`) -- command-level failure,
//!   connection is still usable.
//!
//! Usage: RUST_LOG=info cargo run --example reconnecting_client

use std::time::Duration;

use freeswitch_esl_tokio::prelude::*;
use freeswitch_esl_tokio::{
    EslClient, EslError, EslEvent, EslEventType, EventFormat, DEFAULT_ESL_PASSWORD,
    DEFAULT_ESL_PORT,
};
use tracing::{error, info, warn};

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// sysexits.h EX_CONFIG -- systemd RestartPreventExitStatus=78 keeps the
/// service down on permanent config errors.
const EX_CONFIG: i32 = 78;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let host = std::env::var("ESL_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = std::env::var("ESL_PORT")
        .ok()
        .and_then(|p| {
            p.parse()
                .ok()
        })
        .unwrap_or(DEFAULT_ESL_PORT);
    let password =
        std::env::var("ESL_PASSWORD").unwrap_or_else(|_| DEFAULT_ESL_PASSWORD.to_string());

    let mut backoff = BACKOFF_INITIAL;

    loop {
        info!("connecting to {}:{}", host, port);

        match run_session(&host, port, &password).await {
            Ok(()) => {
                info!("clean disconnect, exiting");
                return;
            }
            Err(EslError::AuthenticationFailed { reason }) => {
                error!("authentication failed: {}", reason);
                std::process::exit(EX_CONFIG);
            }
            Err(EslError::AccessDenied { reason }) => {
                error!("access denied (ACL): {}", reason);
                std::process::exit(EX_CONFIG);
            }
            Err(e) if e.is_connection_error() => {
                warn!("connection lost: {}, retrying in {:?}", e, backoff);
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
            Err(e) => {
                error!("unexpected error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Single ESL session: connect, subscribe, process events until disconnect.
async fn run_session(host: &str, port: u16, password: &str) -> Result<(), EslError> {
    let (client, mut events) = EslClient::connect(host, port, password).await?;
    info!("connected");

    // Reset backoff on successful connection (caller does this implicitly
    // by re-entering the loop, but subscribe failure should not reset it).

    client
        .subscribe_events(
            EventFormat::Plain,
            &[
                EslEventType::ChannelAnswer,
                EslEventType::ChannelHangupComplete,
                EslEventType::Heartbeat,
            ],
        )
        .await?;

    while let Some(result) = events
        .recv()
        .await
    {
        match result {
            Ok(event) => handle_event(&event),
            Err(e) if e.is_recoverable() => {
                // Connection still alive, log and continue
                warn!("recoverable event error: {}", e);
            }
            Err(e) => return Err(e),
        }
    }

    // events.recv() returned None -- reader task exited.
    // Check why the connection closed.
    match events.status() {
        freeswitch_esl_tokio::ConnectionStatus::Disconnected(reason) => {
            info!("disconnected: {:?}", reason);
        }
        status => {
            info!("event stream ended with status: {:?}", status);
        }
    }

    Ok(())
}

fn handle_event(event: &EslEvent) {
    match event.event_type() {
        Some(EslEventType::ChannelAnswer) => {
            let uuid = event
                .unique_id()
                .unwrap_or("?");
            let caller = event
                .caller_id_number()
                .unwrap_or("?");
            let dest = event
                .destination_number()
                .unwrap_or("?");
            info!(
                "{}: {} -> {} answered",
                &uuid[..8.min(uuid.len())],
                caller,
                dest
            );
        }
        Some(EslEventType::ChannelHangupComplete) => {
            let uuid = event
                .unique_id()
                .unwrap_or("?");
            let cause = event
                .hangup_cause()
                .ok()
                .flatten()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into());
            info!("{}: hangup ({})", &uuid[..8.min(uuid.len())], cause);
        }
        Some(EslEventType::Heartbeat) => {
            let sessions = event
                .header(EventHeader::SessionCount)
                .unwrap_or("?");
            info!("heartbeat, sessions: {}", sessions);
        }
        _ => {}
    }
}
