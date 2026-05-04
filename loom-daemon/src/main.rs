// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-daemon
//!
//! Main Loom cryptocurrency arbitrage daemon.
//!
//! Orchestrates initialization: loads configuration, connects to Coinbase FIX,
//! initializes the trading graph and opportunity database, spawns the arbitrage
//! detection event loop, and starts the gRPC server for loomctl clients.
//!
//! ## Usage
//!
//! ```bash
//! loom-daemon --config loom.toml
//! loom-daemon --example-config > loom.toml
//! ```

use clap::Parser;
use loom_config::LoomConfig;
use loom_engine::{ListenConfig, ListenEventLoop};
use loom_fix::market_data::{
    MDUpdateType, MarketDataIncrementalRefresh, MarketDataRequestBuilder, SecurityList,
    SecurityListRequestBuilder, SubscriptionRequestType,
};
use loom_fix::message::MsgType;
use loom_fix::session::client::{ClientSession, SessionConfig, SessionState};
use loom_fix::transport::{ConnectionConfig, FixConnection};
use loom_graph::AdjacencyMatrix;
use loom_server::{start_server, TransportType};
use tokio::select;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "Loom - Cryptocurrency arbitrage engine", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "loom.toml")]
    config: String,

    /// Generate example configuration and exit
    #[arg(long)]
    example_config: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .init();

    let args = Args::parse();

    // Handle example config generation
    if args.example_config {
        let example = LoomConfig::sandbox_example();
        let toml_str = toml::to_string_pretty(&example)?;
        println!("{}", toml_str);
        return Ok(());
    }

    // Load configuration
    info!("Loading configuration from {}", args.config);
    let config = LoomConfig::from_file(&args.config)?;
    info!("Configuration loaded successfully");

    // Initialize graph with placeholder
    // Actual currencies will be discovered via SecurityListRequest after FIX connection
    info!("Initializing placeholder graph (currencies will be discovered from Coinbase)");
    let currencies = vec!["USD".to_string()];
    let graph = AdjacencyMatrix::new(currencies.clone());
    info!("Placeholder graph initialized, awaiting SecurityList from Coinbase");

    // Initialize event loop with database
    info!(
        "Initializing event loop with database at {}",
        config.database.path
    );
    let event_loop_config = ListenConfig {
        safety_margin: config.engine.safety_margin,
        fee_bps: config.engine.fee_bps as u16,
        cycle_check_interval_ms: config.engine.cycle_check_interval_ms,
    };

    // Event loop will be recreated after receiving SecurityList with actual currencies
    let mut event_loop =
        ListenEventLoop::with_database(graph, event_loop_config.clone(), &config.database.path)?;
    info!("Event loop initialized (will be recreated after SecurityList)");

    // Start gRPC server
    info!(
        "Starting gRPC server on {} (transport: {})",
        config.server.address, config.server.transport
    );

    let transport = match config.server.transport.as_str() {
        "unix" => TransportType::Unix(config.server.address.clone()),
        "tcp" => TransportType::Tcp(config.server.address.clone()),
        _ => {
            return Err(format!("Invalid transport type: {}", config.server.transport).into());
        }
    };

    let db_path = config.database.path.clone();
    let mut server_handle = tokio::spawn(async move {
        if let Err(e) = start_server(transport, db_path).await {
            error!("gRPC server error: {}", e);
        }
    });
    info!("gRPC server started");

    // Connect to FIX server
    info!("Connecting to FIX server at {}", config.fix.address);
    let fix_config = ConnectionConfig {
        address: config.fix.address.clone(),
        server_name: config.fix.server_name.clone(),
        max_retries: 10,
        initial_backoff_ms: 1000,
        max_backoff_ms: 60000,
        heartbeat_interval_secs: config.fix.heartbeat_interval,
    };

    let mut fix_connection = FixConnection::new(fix_config)?;

    match fix_connection.connect().await {
        Ok(_) => {
            info!("FIX connection established");

            // Create FIX session
            let session_config = SessionConfig {
                sender_comp_id: config.fix.sender_comp_id.clone(),
                target_comp_id: config.fix.target_comp_id.clone(),
                api_secret: config.fix.api_secret.clone(),
                api_passphrase: config.fix.passphrase.clone(),
                heartbeat_interval: config.fix.heartbeat_interval as u32,
            };

            let mut session = ClientSession::new(session_config);

            // Send Logon message
            info!("Sending Logon message with HMAC authentication");
            let logon_msg = session.create_logon_message();
            let logon_display = String::from_utf8_lossy(&logon_msg).replace('\x01', "|");
            info!("Logon bytes: {}", logon_display);
            fix_connection.send(&logon_msg).await?;
            session.set_state(SessionState::LoggingIn);
            session.update_last_send_time();
            info!("Logon message sent");

            // Create read buffer for FIX messages
            let mut read_buf = vec![0u8; 8192];

            // Create timer for cycle checks
            let mut cycle_check_timer =
                interval(Duration::from_millis(config.engine.cycle_check_interval_ms));

            // Create heartbeat check timer
            let mut heartbeat_timer = interval(Duration::from_secs(
                (config.fix.heartbeat_interval as f64 * 0.75) as u64,
            ));

            // Main event loop
            loop {
                select! {
                    // Read FIX messages from connection
                    result = fix_connection.recv(&mut read_buf) => {
                        match result {
                            Ok(n) if n == 0 => {
                                warn!("FIX connection closed by peer");
                                break;
                            }
                            Ok(n) => {
                                // Feed data to session parser
                                session.feed_data(&read_buf[..n]);

                                // Process all available messages
                                while let Ok(Some(msg)) = session.parse_next_message() {
                                    debug!("Received FIX message: {:?}", msg.msg_type);

                                    // Check sequence number
                                    if let Some(msg_seq_num) = msg.get_field(loom_fix::message::Tag::MsgSeqNum) {
                                        if let Ok(seq_num) = msg_seq_num.parse::<u64>() {
                                            if !session.check_sequence_number(seq_num) {
                                                warn!("Sequence gap detected: expected {}, got {}",
                                                      session.expected_seq_num_in(), seq_num);
                                                // Send ResendRequest
                                                let resend_msg = session.create_resend_request(
                                                    session.expected_seq_num_in(),
                                                    seq_num - 1,
                                                );
                                                fix_connection.send(&resend_msg).await?;
                                                session.update_last_send_time();
                                            }
                                            session.increment_seq_num_in();
                                        }
                                    }

                                    // Handle message by type
                                    match msg.msg_type {
                                        MsgType::Logon => {
                                            info!("Logon accepted");
                                            session.set_state(SessionState::LoggedIn);

                                            // Request list of all available securities from Coinbase
                                            info!("Requesting SecurityList to discover available trading pairs");

                                            let now = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap();
                                            let secs = now.as_secs();
                                            let millis = now.subsec_millis();
                                            let days_since_epoch = secs / 86400;
                                            let secs_today = secs % 86400;
                                            let year = 1970 + (days_since_epoch / 365);
                                            let month = ((days_since_epoch % 365) / 30) + 1;
                                            let day = ((days_since_epoch % 365) % 30) + 1;
                                            let hour = secs_today / 3600;
                                            let minute = (secs_today % 3600) / 60;
                                            let second = secs_today % 60;
                                            let sending_time = format!(
                                                "{:04}{:02}{:02}-{:02}:{:02}:{:02}.{:03}",
                                                year, month, day, hour, minute, second, millis
                                            );

                                            let sec_list_req = SecurityListRequestBuilder::new(
                                                session.current_seq_num_out(),
                                                config.fix.sender_comp_id.clone(),
                                                config.fix.target_comp_id.clone(),
                                                sending_time,
                                                "SECLIST-REQ-1".to_string(),
                                            )
                                            .build();

                                            if let Err(e) = fix_connection.send(&sec_list_req).await {
                                                error!("Failed to send SecurityListRequest: {}", e);
                                            } else {
                                                session.update_last_send_time();
                                                info!("SecurityListRequest sent");
                                            }
                                        }
                                        MsgType::SecurityList => {
                                            info!("Received SecurityList from Coinbase");

                                            if let Some(sec_list) = SecurityList::from_fix_message(&msg) {
                                                info!("SecurityList contains {} trading pairs", sec_list.symbols.len());

                                                // Extract unique currencies from all symbols
                                                let mut currency_set = std::collections::HashSet::new();
                                                for symbol in &sec_list.symbols {
                                                    // Split by '-' to get base and quote currencies
                                                    let parts: Vec<&str> = symbol.split('-').collect();
                                                    if parts.len() == 2 {
                                                        currency_set.insert(parts[0].to_string());
                                                        currency_set.insert(parts[1].to_string());
                                                    }
                                                }

                                                let mut currencies: Vec<String> = currency_set.into_iter().collect();
                                                currencies.sort(); // Sort for deterministic ordering

                                                info!("Discovered {} unique currencies: {:?}", currencies.len(), currencies);

                                                // Recreate graph with discovered currencies
                                                let new_graph = AdjacencyMatrix::new(currencies.clone());
                                                info!("Recreated graph with discovered currencies");

                                                // Recreate event loop with new graph
                                                event_loop = ListenEventLoop::with_database(
                                                    new_graph,
                                                    event_loop_config.clone(),
                                                    &config.database.path,
                                                )?;
                                                info!("Event loop recreated with full currency graph");

                                                // Subscribe to market data for all discovered trading pairs
                                                info!("Subscribing to market data for {} trading pairs", sec_list.symbols.len());

                                                for symbol in &sec_list.symbols {
                                                    let now = std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap();
                                                    let secs = now.as_secs();
                                                    let millis = now.subsec_millis();
                                                    let days_since_epoch = secs / 86400;
                                                    let secs_today = secs % 86400;
                                                    let year = 1970 + (days_since_epoch / 365);
                                                    let month = ((days_since_epoch % 365) / 30) + 1;
                                                    let day = ((days_since_epoch % 365) % 30) + 1;
                                                    let hour = secs_today / 3600;
                                                    let minute = (secs_today % 3600) / 60;
                                                    let second = secs_today % 60;
                                                    let sending_time = format!(
                                                        "{:04}{:02}{:02}-{:02}:{:02}:{:02}.{:03}",
                                                        year, month, day, hour, minute, second, millis
                                                    );

                                                    let md_req = MarketDataRequestBuilder::new(
                                                        session.current_seq_num_out(),
                                                        config.fix.sender_comp_id.clone(),
                                                        config.fix.target_comp_id.clone(),
                                                        sending_time,
                                                        format!("MDREQ-{}", symbol),
                                                    )
                                                    .subscription_request_type(SubscriptionRequestType::Subscribe)
                                                    .market_depth(0) // Full book
                                                    .md_update_type(MDUpdateType::IncrementalRefresh)
                                                    .add_symbol(symbol.to_string())
                                                    .build();

                                                    if let Err(e) = fix_connection.send(&md_req).await {
                                                        error!("Failed to send MarketDataRequest for {}: {}", symbol, e);
                                                    } else {
                                                        session.update_last_send_time();
                                                        debug!("Subscribed to market data for {}", symbol);
                                                    }
                                                }

                                                info!("Market data subscriptions complete");
                                            } else {
                                                error!("Failed to parse SecurityList message");
                                            }
                                        }
                                        MsgType::Heartbeat => {
                                            debug!("Received Heartbeat");
                                        }
                                        MsgType::TestRequest => {
                                            debug!("Received TestRequest");
                                            // Extract TestReqID and respond with Heartbeat
                                            if let Some(test_req_id) = msg.get_field(loom_fix::message::Tag::TestReqID) {
                                                let hb_msg = session.create_heartbeat_message(Some(test_req_id));
                                                fix_connection.send(&hb_msg).await?;
                                                session.update_last_send_time();
                                                debug!("Sent Heartbeat in response to TestRequest");
                                            }
                                        }
                                        MsgType::Logout => {
                                            let reason = msg.get_field(loom_fix::message::Tag::Text)
                                                .unwrap_or("(no reason provided)");
                                            warn!("Received Logout from server: {}", reason);
                                            session.set_state(SessionState::Disconnected);
                                            break; // Exit main loop
                                        }
                                        MsgType::MarketDataIncrementalRefresh => {
                                            // Parse market data update
                                            if let Some(md_refresh) = MarketDataIncrementalRefresh::from_fix_message(&msg) {
                                                debug!("Processing {} market data entries", md_refresh.entries.len());

                                                // Extract symbol from message (Tag 55)
                                                let instrument = msg.get_field(loom_fix::message::Tag::Symbol)
                                                    .unwrap_or("BTC-USD"); // Default to BTC-USD if not present

                                                for entry in &md_refresh.entries {
                                                    event_loop.handle_market_data(entry, instrument);
                                                }
                                            }
                                        }
                                        _ => {
                                            debug!("Unhandled message type: {:?}", msg.msg_type);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error reading from FIX connection: {}", e);
                                break;
                            }
                        }
                    }

                    // Periodic cycle check
                    _ = cycle_check_timer.tick() => {
                        if session.state() == SessionState::LoggedIn {
                            debug!("Checking for arbitrage cycles");
                            event_loop.check_for_cycles();
                        }
                    }

                    // Periodic heartbeat check
                    _ = heartbeat_timer.tick() => {
                        if session.state() == SessionState::LoggedIn && session.should_send_heartbeat() {
                            debug!("Sending heartbeat");
                            let hb_msg = session.create_heartbeat_message(None);
                            if let Err(e) = fix_connection.send(&hb_msg).await {
                                error!("Failed to send heartbeat: {}", e);
                                break;
                            }
                            session.update_last_send_time();
                        }
                    }

                    // Server handle completion (graceful shutdown)
                    _ = &mut server_handle => {
                        info!("gRPC server exited, shutting down");
                        break;
                    }
                }
            }

            // Send Logout before disconnecting
            if session.state() != SessionState::Disconnected {
                info!("Sending Logout message");
                let logout_msg = session.create_logout_message();
                let _ = fix_connection.send(&logout_msg).await;
            }
        }
        Err(e) => {
            error!("Failed to connect to FIX server: {}", e);
            error!("Starting daemon in degraded mode (gRPC only)");
            server_handle.await?;
        }
    }

    Ok(())
}
