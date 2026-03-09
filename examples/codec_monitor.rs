//! Codec and bit rate monitor -- subscribes to CODEC events for live transcoding visibility.
//!
//! FreeSWITCH fires a CODEC event every time a read or write codec is set or changed on a
//! channel. Each event carries `Channel-Read-Codec-Bit-Rate` and `Channel-Write-Codec-Bit-Rate`
//! as event headers (these are NOT channel variables -- `uuid_getvar` cannot retrieve them).
//!
//! This example subscribes to CODEC, CHANNEL_ANSWER, and CHANNEL_BRIDGE events to show:
//! - Initial codec negotiation (on answer)
//! - Codec changes mid-call (re-INVITE, ofer/answer renegotiation)
//! - Bridge pairs with mismatched codecs (transcoding detection)
//!
//! Limitation: for AMR-WB, `bits_per_second` reflects the SDP-negotiated rate, not per-frame
//! mode changes. AMR-WB can switch modes frame-by-frame without firing a new CODEC event.
//!
//! Usage: RUST_LOG=info cargo run --example codec_monitor [-- [host[:port]] [password]]

use std::collections::HashMap;

use freeswitch_esl_tokio::{
    EslClient, EslError, EslEventType, EventFormat, EventHeader, HeaderLookup,
    DEFAULT_ESL_PASSWORD, DEFAULT_ESL_PORT,
};
use tracing::{error, info, warn};

fn short_uuid(uuid: &str) -> &str {
    &uuid[..8.min(uuid.len())]
}

struct CodecInfo {
    read_codec: String,
    read_rate: String,
    read_bitrate: String,
    write_codec: String,
    write_rate: String,
    write_bitrate: String,
}

impl CodecInfo {
    fn from_event(event: &freeswitch_esl_tokio::EslEvent) -> Self {
        Self {
            read_codec: event
                .header(EventHeader::ChannelReadCodecName)
                .unwrap_or("-")
                .to_string(),
            read_rate: event
                .header(EventHeader::ChannelReadCodecRate)
                .unwrap_or("-")
                .to_string(),
            read_bitrate: event
                .header(EventHeader::ChannelReadCodecBitRate)
                .unwrap_or("-")
                .to_string(),
            write_codec: event
                .header(EventHeader::ChannelWriteCodecName)
                .unwrap_or("-")
                .to_string(),
            write_rate: event
                .header(EventHeader::ChannelWriteCodecRate)
                .unwrap_or("-")
                .to_string(),
            write_bitrate: event
                .header(EventHeader::ChannelWriteCodecBitRate)
                .unwrap_or("-")
                .to_string(),
        }
    }

    fn is_transcoding_with(&self, other: &CodecInfo) -> bool {
        self.read_codec != other.read_codec
            || self.read_rate != other.read_rate
            || self.read_bitrate != other.read_bitrate
    }

    fn summary(&self) -> String {
        format!(
            "r={}/{}hz/{}bps w={}/{}hz/{}bps",
            self.read_codec,
            self.read_rate,
            self.read_bitrate,
            self.write_codec,
            self.write_rate,
            self.write_bitrate,
        )
    }
}

struct Monitor {
    channels: HashMap<String, CodecInfo>,
    /// channel UUID -> bridge partner UUID
    bridges: HashMap<String, String>,
}

impl Monitor {
    fn new() -> Self {
        Self {
            channels: HashMap::new(),
            bridges: HashMap::new(),
        }
    }

    fn handle_codec(&mut self, event: &freeswitch_esl_tokio::EslEvent) {
        let uuid = match event.unique_id() {
            Some(u) => u.to_string(),
            None => return,
        };
        let prev = self
            .channels
            .get(&uuid)
            .map(|c| c.summary());
        let info = CodecInfo::from_event(event);
        let current = info.summary();

        match prev {
            Some(ref old) if old == &current => {}
            Some(old) => {
                info!(
                    "{} codec changed: {} -> {}",
                    short_uuid(&uuid),
                    old,
                    current
                );
                self.check_transcoding(&uuid);
            }
            None => {
                info!("{} codec set: {}", short_uuid(&uuid), current);
            }
        }

        self.channels
            .insert(uuid, info);
    }

    fn handle_bridge(&mut self, event: &freeswitch_esl_tokio::EslEvent) {
        let uuid = match event.unique_id() {
            Some(u) => u.to_string(),
            None => return,
        };
        let other = match event.header(EventHeader::OtherLegUniqueId) {
            Some(u) => u.to_string(),
            None => return,
        };

        // Update codec info from the bridge event itself (it carries Channel-* headers)
        let info = CodecInfo::from_event(event);
        self.channels
            .insert(uuid.clone(), info);

        self.bridges
            .insert(uuid.clone(), other.clone());
        self.bridges
            .insert(other.clone(), uuid.clone());

        self.check_transcoding(&uuid);
    }

    fn handle_destroy(&mut self, event: &freeswitch_esl_tokio::EslEvent) {
        let uuid = match event.unique_id() {
            Some(u) => u.to_string(),
            None => return,
        };
        if let Some(partner) = self
            .bridges
            .remove(&uuid)
        {
            self.bridges
                .remove(&partner);
        }
        self.channels
            .remove(&uuid);
    }

    fn check_transcoding(&self, uuid: &str) {
        let partner = match self
            .bridges
            .get(uuid)
        {
            Some(p) => p,
            None => return,
        };
        let (Some(a), Some(b)) = (
            self.channels
                .get(uuid),
            self.channels
                .get(partner),
        ) else {
            return;
        };

        if a.is_transcoding_with(b) {
            warn!(
                "TRANSCODING {}<->{}: A[{}] B[{}]",
                short_uuid(uuid),
                short_uuid(partner),
                a.summary(),
                b.summary(),
            );
        } else {
            info!(
                "passthrough {}<->{}: {}",
                short_uuid(uuid),
                short_uuid(partner),
                a.summary(),
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut args = std::env::args().skip(1);
    let host_port = args
        .next()
        .unwrap_or_else(|| "localhost".to_string());
    let password = args
        .next()
        .unwrap_or_else(|| DEFAULT_ESL_PASSWORD.to_string());

    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .unwrap_or(DEFAULT_ESL_PORT),
        ),
        None => (host_port, DEFAULT_ESL_PORT),
    };

    let (client, mut events) = match EslClient::connect(&host, port, &password).await {
        Ok(pair) => {
            info!("Connected to {}:{}", host, port);
            pair
        }
        Err(EslError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            error!("Connection refused at {}:{}", host, port);
            return Err(e.into());
        }
        Err(e) => return Err(e.into()),
    };

    client
        .subscribe_events(
            EventFormat::Plain,
            &[
                EslEventType::Codec,
                EslEventType::ChannelAnswer,
                EslEventType::ChannelBridge,
                EslEventType::ChannelUnbridge,
                EslEventType::ChannelDestroy,
            ],
        )
        .await?;

    info!("Subscribed to CODEC + bridge lifecycle events");

    let mut monitor = Monitor::new();

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

        match event.event_type() {
            Some(EslEventType::Codec) | Some(EslEventType::ChannelAnswer) => {
                monitor.handle_codec(&event);
            }
            Some(EslEventType::ChannelBridge) => {
                monitor.handle_bridge(&event);
            }
            Some(EslEventType::ChannelUnbridge) | Some(EslEventType::ChannelDestroy) => {
                monitor.handle_destroy(&event);
            }
            _ => {}
        }
    }

    info!("Connection closed");
    client
        .disconnect()
        .await?;

    Ok(())
}
