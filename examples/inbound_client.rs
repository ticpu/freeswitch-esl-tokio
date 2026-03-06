//! Example inbound ESL client
//!
//! This example shows how to connect to FreeSWITCH and execute commands.
//!
//! Usage: cargo run --example inbound_client

use freeswitch_esl_tokio::{EslClient, EslError, DEFAULT_ESL_PASSWORD, DEFAULT_ESL_PORT};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let (client, _events) = match EslClient::connect(&host, port, &password).await {
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

    info!("Executing API commands...");

    match client
        .api("status")
        .await
    {
        Ok(response) => {
            // api_result() strips +OK prefix for action commands,
            // returns raw body for query commands like `status`.
            info!("FreeSWITCH Status:");
            println!(
                "{}",
                response
                    .api_result()
                    .unwrap_or("(empty)")
            );
        }
        Err(e) => error!("Failed to get status: {}", e),
    }

    match client
        .api("show channels count")
        .await
    {
        Ok(response) => {
            info!("Channel Count:");
            println!(
                "{}",
                response
                    .api_result()
                    .unwrap_or("(empty)")
            );
        }
        Err(e) => error!("Failed to get channel count: {}", e),
    }

    match client
        .api("sofia status")
        .await
    {
        Ok(response) => {
            info!("Sofia Status:");
            if let Some(body) = response.body() {
                println!("{}", body);
            }
        }
        Err(e) => error!("Failed to get Sofia status: {}", e),
    }

    match client
        .bgapi("version")
        .await
    {
        Ok(response) => {
            info!("Background API command sent");
            if let Some(job_uuid) = response.job_uuid() {
                info!("Job UUID: {}", job_uuid);
            }
        }
        Err(e) => error!("Failed to execute bgapi: {}", e),
    }

    let vars = ["hostname", "domain", "local_ip_v4", "switch_serial"];
    for var in &vars {
        match client
            .api(&format!("global_getvar {}", var))
            .await
        {
            Ok(response) => {
                if let Some(body) = response.body() {
                    info!("{}: {}", var, body);
                }
            }
            Err(e) => error!("Failed to get {}: {}", var, e),
        }
    }

    info!("Disconnecting...");
    client
        .disconnect()
        .await?;
    info!("Disconnected successfully");

    Ok(())
}
