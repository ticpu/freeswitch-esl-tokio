//! Example ESL event listener
//!
//! This example shows how to subscribe to FreeSWITCH events and process them.
//!
//! Usage: cargo run --example event_listener
//!        cargo run --example event_listener -- -d    # dump raw wire data to stdout

use freeswitch_esl_tokio::{
    EslClient, EslError, EslEventType, EventFormat, EventHeader, EventSubscription, HeaderLookup,
    DEFAULT_ESL_PASSWORD, DEFAULT_ESL_PORT,
};
use std::collections::HashMap;
use std::io::Write;
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dump_raw = std::env::args().any(|a| a == "-d");

    // Direct tracing to stderr so stdout is clean for -d wire dumps
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

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

    let (client, mut events) = match EslClient::connect(&host, port, &password).await {
        Ok(pair) => {
            info!("Successfully connected to FreeSWITCH");
            pair
        }
        Err(EslError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            error!("Failed to connect to FreeSWITCH (ESL_HOST/ESL_PORT to override)");
            return Err(e.into());
        }
        Err(e) => {
            error!("Failed to connect: {}", e);
            return Err(e.into());
        }
    };

    // Build an EventSubscription describing everything we want to receive.
    // apply_subscription() sends filters and the event command in one call.
    let subscription = if dump_raw {
        EventSubscription::all(EventFormat::Plain)
    } else {
        EventSubscription::new(EventFormat::Plain)
            .events(EslEventType::CHANNEL_EVENTS)
            .event(EslEventType::Dtmf)
            .event(EslEventType::Heartbeat)
            .event(EslEventType::BackgroundJob)
    };

    info!("Subscribing to events...");
    client
        .apply_subscription(&subscription)
        .await?;

    let mut active_calls: HashMap<String, CallInfo> = HashMap::new();
    let mut event_count = 0u64;
    let stdout = std::io::stdout();

    info!("Listening for events... Press Ctrl+C to exit");

    while let Some(result) = events
        .recv()
        .await
    {
        let event = match result {
            Ok(event) => event,
            Err(e) => {
                error!("Event error: {}", e);
                continue;
            }
        };
        event_count += 1;
        debug!("Received event #{}: {:?}", event_count, event.event_type());

        if dump_raw {
            let mut out = stdout.lock();
            let _ = out.write_all(
                event
                    .to_plain_format()
                    .as_bytes(),
            );
        }

        if let Err(e) = process_event(&event, &mut active_calls) {
            error!("Error processing event: {}", e);
        }
    }

    info!("Connection closed, total events: {}", event_count);
    client
        .disconnect()
        .await?;

    Ok(())
}

fn process_event(
    event: &freeswitch_esl_tokio::EslEvent,
    active_calls: &mut HashMap<String, CallInfo>,
) -> Result<(), EslError> {
    match event.event_type() {
        Some(EslEventType::ChannelCreate) => {
            if let Some(uuid) = event.unique_id() {
                let caller_id = event
                    .caller_id_number()
                    .unwrap_or("Unknown");
                let destination = event
                    .header(EventHeader::CallerDestinationNumber)
                    .unwrap_or("Unknown");
                let direction = match event.call_direction() {
                    Ok(Some(d)) => d.to_string(),
                    Ok(None) => "Unknown".into(),
                    Err(e) => format!("!ERR({})", e),
                };

                let call_info = CallInfo {
                    caller_id: caller_id.to_string(),
                    start_time: std::time::Instant::now(),
                    answered_time: None,
                };

                info!("New call: {} -> {} ({})", caller_id, destination, direction);
                active_calls.insert(uuid.to_string(), call_info);
            }
        }
        Some(EslEventType::ChannelAnswer) => {
            if let Some(uuid) = event.unique_id() {
                if let Some(call_info) = active_calls.get_mut(uuid) {
                    call_info.answered_time = Some(std::time::Instant::now());
                    let duration = call_info
                        .start_time
                        .elapsed();
                    info!(
                        "Call answered: {} (ring time: {:.2}s)",
                        call_info.caller_id,
                        duration.as_secs_f64()
                    );
                }
            }
        }
        Some(EslEventType::ChannelHangup) => {
            if let Some(uuid) = event.unique_id() {
                if let Some(call_info) = active_calls.get(uuid) {
                    let cause = match event.hangup_cause() {
                        Ok(Some(c)) => c.to_string(),
                        _ => "UNKNOWN".into(),
                    };
                    let talk_time = call_info
                        .answered_time
                        .map(|t| t.elapsed());

                    if let Some(talk_duration) = talk_time {
                        info!(
                            "Call ended: {} (cause: {}, talk time: {:.2}s)",
                            call_info.caller_id,
                            cause,
                            talk_duration.as_secs_f64()
                        );
                    } else {
                        info!(
                            "Call ended: {} (cause: {}, not answered)",
                            call_info.caller_id, cause
                        );
                    }
                }
            }
        }
        Some(EslEventType::ChannelHangupComplete) => {
            if let Some(uuid) = event.unique_id() {
                active_calls.remove(uuid);
                debug!("Cleaned up call: {}", uuid);
            }
        }
        Some(EslEventType::Dtmf) => {
            if let (Some(uuid), Some(digit)) =
                (event.unique_id(), event.header(EventHeader::DtmfDigit))
            {
                info!("DTMF: {} pressed '{}'", uuid, digit);
            }
        }
        Some(EslEventType::Heartbeat) => {
            if let Some(sessions) = event.header(EventHeader::SessionCount) {
                info!("Heartbeat - active sessions: {}", sessions);
            }
        }
        Some(EslEventType::BackgroundJob) => {
            if let Some(job_uuid) = event.job_uuid() {
                info!("Background job completed: {}", job_uuid);
                if let Some(body) = event.body() {
                    debug!("Job result: {}", body);
                }
            }
        }
        _ => {
            debug!("Ignoring event type: {:?}", event.event_type());
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct CallInfo {
    caller_id: String,
    start_time: std::time::Instant,
    answered_time: Option<std::time::Instant>,
}
