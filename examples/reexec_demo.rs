//! Re-exec teardown and adopt demo
//!
//! Demonstrates the full re-exec cycle without actually calling exec():
//! 1. Connect to FreeSWITCH, subscribe to events
//! 2. Teardown: stop the reader, get the fd and residual bytes
//! 3. Adopt: reconstruct a new EslClient from the same fd
//! 4. Verify events still flow on the adopted connection
//!
//! Requires FreeSWITCH ESL on localhost:8021 (password ClueCon).
//!
//! Usage: cargo run --example reexec_demo

#[cfg(unix)]
mod demo {
    use freeswitch_esl_tokio::{EslClient, EslError, EslEventType, EventFormat, DEFAULT_ESL_PORT};
    use std::os::unix::io::FromRawFd;
    use tracing::{error, info};

    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        tracing_subscriber::fmt::init();

        let port: u16 = std::env::var("ESL_PORT")
            .ok()
            .and_then(|s| {
                s.parse()
                    .ok()
            })
            .unwrap_or(DEFAULT_ESL_PORT);

        // Phase 1: connect and subscribe
        let (client, mut events) = match EslClient::connect("localhost", port, "ClueCon").await {
            Ok(pair) => {
                info!("Connected to FreeSWITCH on port {}", port);
                pair
            }
            Err(EslError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                error!("FreeSWITCH not running on localhost:{}", port);
                return Err(e.into());
            }
            Err(e) => return Err(e.into()),
        };

        client
            .subscribe_events(EventFormat::Plain, &[EslEventType::Heartbeat])
            .await?;
        info!("Subscribed to HEARTBEAT events");

        // Wait for one heartbeat to confirm the subscription works
        info!("Waiting for a heartbeat before teardown...");
        match tokio::time::timeout(std::time::Duration::from_secs(25), events.recv()).await {
            Ok(Some(Ok(event))) => {
                info!(
                    "Got heartbeat: {}",
                    event
                        .header_str("Event-Info")
                        .unwrap_or("(no info)")
                );
            }
            Ok(Some(Err(e))) => error!("Event error: {}", e),
            Ok(None) => error!("Event stream closed"),
            Err(_) => error!("Timed out waiting for heartbeat"),
        }
        drop(events);

        // Phase 2: teardown
        info!("Tearing down for re-exec...");
        let (fd, residual) = client
            .teardown_for_reexec()
            .await?;
        info!(
            "Teardown complete: fd={}, {} residual bytes",
            fd,
            residual.len()
        );

        // In a real re-exec scenario, you would:
        //   1. Serialize app state + residual to disk/env
        //   2. Clear CLOEXEC: nix::fcntl::fcntl(fd, F_SETFD(FdFlag::empty()))
        //   3. std::mem::forget(client)
        //   4. exec() the new binary
        //
        // Here we simulate the new process side without exec().
        //
        // Demo caveat: in a real exec(), mem::forget(client) is sufficient
        // because the old tokio reactor (epoll fd) has CLOEXEC and is gone
        // after exec. In this same-process demo, the reactor still has the
        // original fd registered. We dup() to get a clean fd not known to
        // the reactor, then forget the client (leaks the old registration,
        // but keeps the TCP connection alive by not sending FIN).
        let dup_fd = nix::unistd::dup(fd).expect("dup failed");
        std::mem::forget(client);

        // Phase 3: adopt the stream (simulating new process)
        info!("Adopting stream (simulating new process)...");

        // Reconstruct a TcpStream from the dup'd fd.
        // Safety: dup_fd is exclusively ours, not registered with any reactor.
        let std_stream = unsafe { std::net::TcpStream::from_raw_fd(dup_fd) };
        std_stream.set_nonblocking(true)?;
        let tokio_stream = tokio::net::TcpStream::from_std(std_stream)?;

        let (_new_client, mut new_events) = EslClient::adopt_stream(tokio_stream, &residual)?;
        info!("Adopted stream, waiting for events...");

        // The old subscription survives on the TCP connection, so
        // heartbeats should arrive without re-subscribing.
        info!("Waiting for heartbeat on adopted connection...");
        match tokio::time::timeout(std::time::Duration::from_secs(25), new_events.recv()).await {
            Ok(Some(Ok(event))) => {
                info!(
                    "Got heartbeat on adopted connection: {}",
                    event
                        .header_str("Event-Info")
                        .unwrap_or("(no info)")
                );
            }
            Ok(Some(Err(e))) => error!("Event error: {}", e),
            Ok(None) => error!("Event stream closed"),
            Err(_) => error!("Timed out waiting for heartbeat on adopted connection"),
        }

        info!("Re-exec demo complete");
        Ok(())
    }
}

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    demo::run().await
}

#[cfg(not(unix))]
fn main() {
    eprintln!("reexec_demo requires unix (raw fd support)");
    std::process::exit(1);
}
