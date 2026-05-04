// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loomctl
//!
//! Command-line client for the Loom cryptocurrency arbitrage engine.
//!
//! Connects to loom-daemon via gRPC over a Unix socket or TCP endpoint.
//! Provides subcommands to query engine status, list trading pairs, discover
//! arbitrage opportunities, and inspect individual opportunity details.
//!
//! ## Usage
//!
//! ```bash
//! loomctl --server unix:///tmp/loom.sock status
//! loomctl pairs
//! loomctl opportunities
//! loomctl opportunity <id>
//! ```

use clap::{Parser, Subcommand};
use loom_proto::loom_control_client::LoomControlClient;
use loom_proto::*;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

#[derive(Parser)]
#[command(name = "loom")]
#[command(about = "Loom cryptocurrency arbitrage engine CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "http://[::1]:50051")]
    server: String,

    #[arg(long)]
    json: bool,
}

async fn create_client(
    server: &str,
) -> Result<LoomControlClient<Channel>, Box<dyn std::error::Error>> {
    if server.starts_with("unix://") || server.starts_with("/") {
        // Unix socket
        let path = server.strip_prefix("unix://").unwrap_or(server).to_string();

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| {
                let path = path.clone();
                async move {
                    let stream = tokio::net::UnixStream::connect(path).await?;
                    Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
                }
            }))
            .await?;

        Ok(LoomControlClient::new(channel))
    } else {
        // TCP
        Ok(LoomControlClient::connect(server.to_string()).await?)
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Show daemon status
    Status,

    /// Get current mode
    Mode,

    /// List lump sums
    Ls,

    /// List opportunities
    Opportunities {
        #[arg(short, long, default_value = "10")]
        limit: u32,

        #[arg(long)]
        start_time: Option<u64>,

        #[arg(long)]
        end_time: Option<u64>,

        #[arg(long)]
        gated_only: bool,

        #[arg(long)]
        ungated_only: bool,
    },

    /// Show graph state
    Graph,

    /// Stream real-time events
    Events {
        #[arg(short, long)]
        filter: Vec<String>,
    },

    /// Shutdown daemon
    Shutdown,

    /// Export ticks data
    ExportTicks {
        #[arg(long)]
        start_time: Option<u64>,

        #[arg(long)]
        end_time: Option<u64>,

        #[arg(long)]
        instrument: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut client = create_client(&cli.server).await?;

    match cli.command {
        Commands::Status => {
            let response = client.get_status(StatusRequest {}).await?.into_inner();

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Mode: {}", response.mode);
                println!("Connected: {}", response.connected);
                println!("Uptime: {}s", response.uptime_secs);
                println!("Opportunities: {}", response.opportunities_detected);
                println!("Ticks: {}", response.ticks_processed);
            }
        }

        Commands::Mode => {
            let response = client.get_mode(GetModeRequest {}).await?.into_inner();
            println!("{}", response.mode);
        }

        Commands::Ls => {
            let response = client
                .list_lump_sums(ListLumpSumsRequest {})
                .await?
                .into_inner();

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                for ls in response.lump_sums {
                    println!(
                        "{}: {} {} @ {}",
                        ls.name, ls.current_size, ls.current_currency, ls.state
                    );
                }
            }
        }

        Commands::Opportunities {
            limit,
            start_time,
            end_time,
            gated_only,
            ungated_only,
        } => {
            let response = client
                .list_opportunities(ListOpportunitiesRequest {
                    limit,
                    start_time: start_time.unwrap_or(0),
                    end_time: end_time.unwrap_or(0),
                    gated_only,
                    ungated_only,
                })
                .await?
                .into_inner();

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                for opp in response.opportunities {
                    println!(
                        "{}: {:?} profit={:.4} gated={}",
                        opp.id, opp.cycle, opp.expected_profit, opp.gated
                    );
                }
            }
        }

        Commands::Graph => {
            let response = client
                .get_graph_state(GraphStateRequest {})
                .await?
                .into_inner();

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Nodes: {}", response.node_count);
                println!("Edges: {}", response.edge_count);
                println!("Currencies: {:?}", response.currencies);
            }
        }

        Commands::Events { filter } => {
            let mut stream = client
                .stream_events(EventStreamRequest {
                    event_types: filter,
                })
                .await?
                .into_inner();

            while let Some(event) = stream.message().await? {
                if cli.json {
                    println!("{}", serde_json::to_string(&event)?);
                } else {
                    println!("[{}] {}: {}", event.timestamp, event.event_type, event.data);
                }
            }
        }

        Commands::Shutdown => {
            let response = client.shutdown(ShutdownRequest {}).await?.into_inner();
            if response.success {
                println!("Daemon shutting down...");
            } else {
                eprintln!("Shutdown failed");
            }
        }

        Commands::ExportTicks {
            start_time,
            end_time,
            instrument,
        } => {
            let response = client
                .export_ticks(ExportTicksRequest {
                    start_time: start_time.unwrap_or(0),
                    end_time: end_time.unwrap_or(0),
                    instrument: instrument.unwrap_or_default(),
                })
                .await?
                .into_inner();

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Exported {} ticks", response.ticks.len());
            }
        }
    }

    Ok(())
}
