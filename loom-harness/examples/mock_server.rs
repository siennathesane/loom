// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Example of running a mock Coinbase FIX server for testing.
//!
//! This example starts a mock FIX server on localhost:9878 that simulates
//! Coinbase's FIX protocol behavior.
//!
//! Run with:
//! ```
//! cargo run --example mock_server -p loom-harness
//! ```

use loom_harness::server::{MockCoinbaseServer, MockServerConfig};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Starting Mock Coinbase FIX Server...");

    // Create server with default config
    let mut server = MockCoinbaseServer::new(MockServerConfig {
        server_comp_id: "Coinbase".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_passphrase".to_string(),
        heartbeat_interval: 30,
    });

    // Start listening
    server.start("127.0.0.1:9878").await?;
    let addr = server.local_addr().unwrap();
    println!("Mock server listening on {}", addr);

    // Configure available instruments
    server
        .configure_market_data(vec![
            "BTC-USD".to_string(),
            "ETH-USD".to_string(),
            "SOL-USD".to_string(),
        ])
        .await;

    println!("Configured instruments: BTC-USD, ETH-USD, SOL-USD");
    println!("Ready for FIX client connections!");
    println!();
    println!("Expected credentials:");
    println!("  - API Secret: test_secret");
    println!("  - API Passphrase: test_passphrase");
    println!();
    println!("Press Ctrl+C to stop");

    // Run server
    server.run().await
}
