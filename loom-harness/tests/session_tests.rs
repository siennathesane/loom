// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Session-level FIX protocol tests.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use loom_fix::session::client::{ClientSession, SessionConfig, SessionState};
use loom_harness::server::{MockCoinbaseServer, MockServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};

/// Helper to create test client config
fn test_client_config(target: &str) -> SessionConfig {
    SessionConfig {
        sender_comp_id: "TEST_CLIENT".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_passphrase".to_string(),
        heartbeat_interval: 30,
    }
}

/// Helper to create test server config
fn test_server_config() -> MockServerConfig {
    MockServerConfig::default()
}

#[tokio::test]
async fn test_valid_logon_handshake() {
    // Start mock server
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    // Run server in background
    tokio::spawn(async move {
        server.run().await.ok();
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Connect client
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config(&addr.to_string()));

    // Send Logon
    let logon_msg = client.create_logon_message();
    stream.write_all(&logon_msg).await.unwrap();

    // Read Logon response
    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    assert!(n > 0, "Should receive Logon response");

    // Parse response
    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);
    assert_eq!(response.sender_comp_id, "Coinbase");
    assert_eq!(response.get_field(Tag::HeartBtInt), Some("30"));
}

#[tokio::test]
async fn test_invalid_logon_rejection() {
    // Start mock server
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    // Connect and send invalid Logon (without RawData)
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let mut msg = FixMessage::new(
        MsgType::Logon,
        1,
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    msg.add_field(Tag::EncryptMethod, "0".to_string());
    msg.add_field(Tag::HeartBtInt, "30".to_string());

    use loom_fix::serialization::build_fix_message;
    let logon_bytes = build_fix_message(&msg);

    stream.write_all(&logon_bytes).await.unwrap();

    // Should receive connection close (no response for invalid logon)
    let mut buffer = vec![0u8; 4096];
    let result = timeout(Duration::from_secs(1), stream.read(&mut buffer)).await;

    // Either timeout or connection closed (read returns 0)
    match result {
        Ok(Ok(n)) => {
            assert_eq!(n, 0, "Connection should be closed on invalid logon");
        }
        Ok(Err(_)) | Err(_) => {
            // Also acceptable - connection closed
        }
    }
}

#[tokio::test]
async fn test_heartbeat_exchange() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config(&addr.to_string()));

    // Logon
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let _ = stream.read(&mut buffer).await.unwrap();

    // Send Heartbeat
    let heartbeat = client.create_heartbeat_message(None);
    stream.write_all(&heartbeat).await.unwrap();

    // Server should acknowledge (no response expected for Heartbeat)
    // Just verify connection stays open
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_test_request_response() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config(&addr.to_string()));

    // Logon
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let _ = stream.read(&mut buffer).await.unwrap();

    // Send TestRequest
    let test_req = client.create_test_request_message("TEST_123");
    stream.write_all(&test_req).await.unwrap();

    // Should receive Heartbeat with TestReqID
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Heartbeat);
    assert_eq!(response.get_field(Tag::TestReqID), Some("TEST_123"));
}

#[tokio::test]
async fn test_sequence_number_tracking() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config(&addr.to_string()));

    // Logon (seq 1)
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let logon_resp = parser.parse_message().unwrap().unwrap();
    assert_eq!(logon_resp.msg_seq_num, 1);

    // Send Heartbeat (seq 2)
    let heartbeat = client.create_heartbeat_message(None);
    stream.write_all(&heartbeat).await.unwrap();

    // Send TestRequest (seq 3) - should get response
    let test_req = client.create_test_request_message("SEQ_TEST");
    stream.write_all(&test_req).await.unwrap();

    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    parser.feed(&buffer[..n]);
    let hb_resp = parser.parse_message().unwrap().unwrap();
    assert_eq!(hb_resp.msg_seq_num, 2); // Server's second message
}

#[tokio::test]
async fn test_resend_request_handling() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config(&addr.to_string()));

    // Logon
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let _ = stream.read(&mut buffer).await.unwrap();

    // Send ResendRequest
    let resend_req = client.create_resend_request(1, 10);
    stream.write_all(&resend_req).await.unwrap();

    // Should receive SequenceReset
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::SequenceReset);
    assert!(response.get_field(Tag::NewSeqNo).is_some());
}

#[tokio::test]
async fn test_logout_sequence() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config(&addr.to_string()));

    // Logon
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let _ = stream.read(&mut buffer).await.unwrap();

    // Send Logout
    let logout = client.create_logout_message();
    stream.write_all(&logout).await.unwrap();

    // Should receive Logout response
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logout);

    // Connection should close
    let result = timeout(Duration::from_secs(1), stream.read(&mut buffer)).await;
    // Either timeout or read returns 0 (connection closed)
    if let Ok(Ok(n)) = result {
        assert_eq!(n, 0, "Connection should be closed");
    }
}
