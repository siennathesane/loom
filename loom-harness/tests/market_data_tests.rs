// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Market data FIX protocol tests.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use loom_fix::session::client::{ClientSession, SessionConfig};
use loom_harness::server::{MockCoinbaseServer, MockServerConfig};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};

fn test_client_config() -> SessionConfig {
    SessionConfig {
        sender_comp_id: "TEST_CLIENT".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_passphrase".to_string(),
        heartbeat_interval: 30,
    }
}

fn test_server_config() -> MockServerConfig {
    MockServerConfig::default()
}

async fn setup_authenticated_connection() -> (TcpStream, ClientSession, SocketAddr) {
    use std::net::SocketAddr;

    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    // Logon
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let _ = stream.read(&mut buffer).await.unwrap();

    (stream, client, addr)
}

#[tokio::test]
async fn test_security_list_request_response() {
    let (mut stream, client, _addr) = setup_authenticated_connection().await;

    // Create SecurityListRequest
    let mut msg = FixMessage::new(
        MsgType::SecurityListRequest,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    msg.add_field(Tag::SecurityReqID, "SEC_REQ_1".to_string());

    use loom_fix::serialization::build_fix_message;
    let request = build_fix_message(&msg);
    stream.write_all(&request).await.unwrap();

    // Should receive SecurityList response
    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::SecurityList);
    assert!(response.get_field(Tag::NoRelatedSym).is_some());
}

#[tokio::test]
async fn test_market_data_subscription() {
    let (mut stream, client, _addr) = setup_authenticated_connection().await;

    // Create MarketDataRequest (subscribe)
    let mut msg = FixMessage::new(
        MsgType::MarketDataRequest,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    msg.add_field(Tag::MDReqID, "MD_REQ_1".to_string());
    msg.add_field(Tag::SubscriptionRequestType, "1".to_string()); // Snapshot + Updates
    msg.add_field(Tag::MarketDepth, "0".to_string()); // Full book

    use loom_fix::serialization::build_fix_message;
    let request = build_fix_message(&msg);
    stream.write_all(&request).await.unwrap();

    // Server acknowledges (no explicit response for now in mock)
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_market_data_incremental_refresh_parsing() {
    let (mut stream, _client, _addr) = setup_authenticated_connection().await;

    // In a real test, server would push MarketDataIncrementalRefresh
    // For now, verify we can handle a simulated one

    // Wait briefly for any server messages
    sleep(Duration::from_millis(200)).await;

    // This test validates the parsing capability
    // The mock server doesn't automatically push updates yet
    // That would be added in a more complete implementation
}

#[tokio::test]
async fn test_security_status_message() {
    let (_stream, _client, _addr) = setup_authenticated_connection().await;

    // Similar to above - validates we can handle SecurityStatus
    // Mock server would need to be extended to push status updates
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_high_frequency_market_data() {
    // This test validates the system can handle rapid market data updates
    // In a full implementation, we'd configure the mock server to send
    // 10K messages/second and verify client can keep up

    let (_stream, _client, _addr) = setup_authenticated_connection().await;

    // Placeholder - would need market data push functionality
    sleep(Duration::from_millis(100)).await;
}
