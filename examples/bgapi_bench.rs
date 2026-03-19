//! bgapi throughput benchmark
//!
//! Sends N `bgapi status` commands as fast as possible and collects all
//! BACKGROUND_JOB results. Measures send-phase latency and full round-trip
//! (send → BACKGROUND_JOB event arrival).
//!
//! Environment variables:
//! - `ESL_HOST` / `ESL_PORT` / `ESL_PASSWORD` -- connection parameters
//! - `BENCH_COUNT` -- number of bgapi commands (default: 1000)
//!
//! Run with: `cargo run --release --example bgapi_bench`

use std::collections::HashMap;
use std::time::{Duration, Instant};

use freeswitch_esl_tokio::{
    EslClient, EslEventType, EventFormat, HeaderLookup, DEFAULT_ESL_PASSWORD, DEFAULT_ESL_PORT,
};
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let n: usize = std::env::var("BENCH_COUNT")
        .ok()
        .and_then(|v| {
            v.parse()
                .ok()
        })
        .unwrap_or(1000);

    let (client, mut events) = EslClient::connect(&host, port, &password).await?;

    // Scale timeout with N so large runs don't time out
    let timeout_ms = 5000 + (n as u64 * 50);
    client.set_command_timeout(Duration::from_millis(timeout_ms));

    client
        .api("status")
        .await?;

    client
        .subscribe_events(EventFormat::Plain, [EslEventType::BackgroundJob])
        .await?;

    // Collector records (job_uuid -> arrival_instant)
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let collector = tokio::spawn(async move {
        let mut arrivals: HashMap<String, Instant> = HashMap::with_capacity(n);
        let mut done_signal = done_rx;
        let mut sends_complete = false;

        loop {
            tokio::select! {
                biased;
                event = events.recv() => {
                    match event {
                        Some(Ok(ev)) => {
                            if let Some(uuid) = ev.job_uuid() {
                                arrivals.insert(uuid.to_string(), Instant::now());
                                if arrivals.len() >= n {
                                    break;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            eprintln!("event error: {e}");
                        }
                        None => break,
                    }
                }
                _ = &mut done_signal, if !sends_complete => {
                    sends_complete = true;
                    // drain remaining events
                }
            }
        }
        arrivals
    });

    let mut send_times: Vec<(String, Instant, Duration)> = Vec::with_capacity(n);
    let send_phase_start = Instant::now();

    for _ in 0..n {
        let t0 = Instant::now();
        let resp = client
            .bgapi("status")
            .await?;
        let elapsed = t0.elapsed();
        let uuid = resp
            .job_uuid()
            .expect("bgapi always returns Job-UUID")
            .to_string();
        send_times.push((uuid, t0, elapsed));
    }

    let send_phase = send_phase_start.elapsed();

    let _ = done_tx.send(());

    let recv_phase_start = Instant::now();
    let arrivals = collector.await?;
    let recv_phase = recv_phase_start.elapsed();

    let total = send_phase + recv_phase;

    let mut send_lats: Vec<Duration> = send_times
        .iter()
        .map(|(_, _, d)| *d)
        .collect();
    send_lats.sort();

    let mut rtts: Vec<Duration> = send_times
        .iter()
        .filter_map(|(uuid, sent_at, _)| {
            arrivals
                .get(uuid)
                .map(|arrived| arrived.duration_since(*sent_at))
        })
        .collect();
    rtts.sort();

    let received = rtts.len();

    println!("bench=rust n={n}");
    println!("received={received}");
    println!("send_phase_ms={}", send_phase.as_millis());
    println!(
        "send_rate_per_sec={:.1}",
        n as f64 / send_phase.as_secs_f64()
    );
    print_latencies("send_lat", &send_lats);
    println!("recv_phase_ms={}", recv_phase.as_millis());
    if !rtts.is_empty() {
        print_latencies("rtt", &rtts);
    }
    println!("total_ms={}", total.as_millis());

    client
        .disconnect()
        .await?;
    Ok(())
}

fn print_latencies(prefix: &str, sorted: &[Duration]) {
    if sorted.is_empty() {
        return;
    }
    let n = sorted.len();
    println!("{prefix}_min_us={}", sorted[0].as_micros());
    println!("{prefix}_median_us={}", sorted[n / 2].as_micros());
    println!(
        "{prefix}_p95_us={}",
        sorted[(n as f64 * 0.95) as usize].as_micros()
    );
    println!(
        "{prefix}_p99_us={}",
        sorted[(n as f64 * 0.99) as usize].as_micros()
    );
    println!("{prefix}_max_us={}", sorted[n - 1].as_micros());
}
