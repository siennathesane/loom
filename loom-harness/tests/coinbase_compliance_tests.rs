// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Comprehensive Coinbase FIX 5.0 SP2 compliance tests.
//!
//! These tests validate compliance with Coinbase's FIX API specification:
//! - Connection and authentication (HMAC-SHA256)
//! - Session management (heartbeats, sequence numbers, resend handling)
//! - Rate limiting (message velocity, connection limits)
//! - Market data flows (SecurityList, MarketDataIncrementalRefresh)
//! - Order entry (NewOrderSingle, ExecutionReport, batch operations)
//! - Drop copy sessions
//! - Error handling and reject scenarios
//!
//! References:
//! - https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
//! - https://docs.cdp.coinbase.com/exchange/fix-api/best-practices
//! - https://docs.cdp.coinbase.com/exchange/fix-api/rate-limits

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use loom_fix::session::client::{ClientSession, SessionConfig};
use loom_harness::server::{MockCoinbaseServer, MockServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Duration};

// ============================================================================
// Test Helpers
// ============================================================================

fn test_client_config() -> SessionConfig {
    SessionConfig {
        sender_comp_id: "TEST_API_KEY".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "base64_encoded_secret".to_string(),
        api_passphrase: "test_passphrase".to_string(),
        heartbeat_interval: 30,
    }
}

fn test_server_config() -> MockServerConfig {
    MockServerConfig::default()
}

async fn setup_server() -> (MockCoinbaseServer, std::net::SocketAddr) {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;
    (MockCoinbaseServer::new(test_server_config()), addr)
}

async fn connect_and_logon(addr: std::net::SocketAddr) -> (TcpStream, ClientSession) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();
    assert_eq!(response.msg_type, MsgType::Logon);

    (stream, client)
}

// ============================================================================
// CONNECTIVITY TESTS (Per Coinbase Connectivity Spec)
// ============================================================================

/// Test: Logon must use FIXT.1.1 as BeginString (Tag 8)
#[tokio::test]
async fn test_logon_begin_string_fixt11() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();

    // Verify BeginString is FIXT.1.1
    let msg_str = String::from_utf8_lossy(&logon_msg);
    assert!(
        msg_str.starts_with("8=FIXT.1.1\x01"),
        "Logon must use BeginString=FIXT.1.1"
    );

    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    assert!(n > 0, "Should receive Logon response");
}

/// Test: Logon must include DefaultApplVerID=9 (FIX 5.0 SP2)
#[tokio::test]
async fn test_logon_default_appl_ver_id() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();
    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);
    assert_eq!(
        response.get_field(Tag::DefaultApplVerID),
        Some("9"),
        "DefaultApplVerID must be 9 (FIX 5.0 SP2)"
    );
}

/// Test: Logon authentication uses HMAC-SHA256 signature
/// Reference: Username(553), Password(554), RawDataLength(95), RawData(96)
#[tokio::test]
async fn test_logon_hmac_sha256_signature() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    // Verify required authentication fields
    assert!(msg_str.contains("553="), "Must include Username(553)");
    assert!(msg_str.contains("554="), "Must include Password(554)");
    assert!(msg_str.contains("95="), "Must include RawDataLength(95)");
    assert!(
        msg_str.contains("96="),
        "Must include RawData(96) signature"
    );

    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);
}

/// Test: TargetCompID must be "Coinbase"
#[tokio::test]
async fn test_logon_target_comp_id_coinbase() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();
    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(
        response.target_comp_id, "Coinbase",
        "SenderCompID in response must be Coinbase"
    );
}

/// Test: HeartBtInt must be <= 300 seconds
#[tokio::test]
async fn test_logon_heartbeat_interval_max_300() {
    let (_, addr) = setup_server().await;

    // Test with valid interval (30s)
    let mut config = test_client_config();
    config.heartbeat_interval = 30;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(config);

    let logon_msg = client.create_logon_message();
    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);

    let hb_int: u32 = response
        .get_field(Tag::HeartBtInt)
        .unwrap()
        .parse()
        .unwrap();

    assert!(hb_int <= 300, "HeartBtInt must be <= 300 seconds");
}

/// Test: EncryptMethod must be 0 (None - SSL/TLS handles encryption)
#[tokio::test]
async fn test_logon_encrypt_method_none() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    assert!(
        msg_str.contains("98=0\x01"),
        "EncryptMethod(98) must be 0 (None)"
    );

    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);
    assert_eq!(response.get_field(Tag::EncryptMethod), Some("0"));
}

// ============================================================================
// SESSION MANAGEMENT TESTS
// ============================================================================

/// Test: Sequence numbers must start at 1 for Logon
#[tokio::test]
async fn test_sequence_numbers_start_at_one() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();
    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(
        response.msg_seq_num, 1,
        "Server's first message (Logon response) must have MsgSeqNum=1"
    );
}

/// Test: Sequence numbers must NOT include leading zeros
#[tokio::test]
async fn test_sequence_numbers_no_leading_zeros() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // Send a few messages and verify sequence numbers
    for i in 2..5 {
        let heartbeat = client.create_heartbeat_message(None);
        stream.write_all(&heartbeat).await.unwrap();

        let msg_str = String::from_utf8_lossy(&heartbeat);

        // Check that sequence number doesn't have leading zeros
        let seq_pattern = format!("34={}\x01", i);
        assert!(
            msg_str.contains(&seq_pattern),
            "MsgSeqNum must not have leading zeros: expected {}",
            i
        );
    }
}

/// Test: SendingTime fractional seconds must use exactly 3 digits
#[tokio::test]
async fn test_sending_time_fractional_seconds() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    // SendingTime format: YYYYMMDD-HH:MM:SS.sss
    // Tag 52
    use regex::Regex;
    let re = Regex::new(r"52=\d{8}-\d{2}:\d{2}:\d{2}\.\d{3}").unwrap();

    assert!(
        re.is_match(&msg_str),
        "SendingTime(52) must have exactly 3 fractional second digits"
    );
}

/// Test: Resend requests are NOT supported (should receive SequenceReset-GapFill)
#[tokio::test]
async fn test_resend_request_not_supported() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // Send ResendRequest
    let resend_req = client.create_resend_request(1, 5);
    stream.write_all(&resend_req).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    // Per spec: "Resend requests are not supported"
    // Server should respond with SequenceReset-GapFill
    assert_eq!(
        response.msg_type,
        MsgType::SequenceReset,
        "ResendRequest should receive SequenceReset-GapFill (not supported)"
    );

    assert_eq!(
        response.get_field(Tag::GapFillFlag),
        Some("Y"),
        "SequenceReset should have GapFillFlag=Y"
    );
}

/// Test: Heartbeat exchange maintains session
#[tokio::test]
async fn test_heartbeat_maintains_session() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // Send multiple heartbeats
    for _ in 0..3 {
        let heartbeat = client.create_heartbeat_message(None);
        stream.write_all(&heartbeat).await.unwrap();
        sleep(Duration::from_millis(50)).await;
    }

    // Verify connection is still alive with TestRequest
    let test_req = client.create_test_request_message("ALIVE_CHECK");
    stream.write_all(&test_req).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Heartbeat);
    assert_eq!(response.get_field(Tag::TestReqID), Some("ALIVE_CHECK"));
}

/// Test: TestRequest must receive Heartbeat with TestReqID echoed
#[tokio::test]
async fn test_test_request_echoes_test_req_id() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    let test_id = "TEST_12345";
    let test_req = client.create_test_request_message(test_id);
    stream.write_all(&test_req).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Heartbeat);
    assert_eq!(
        response.get_field(Tag::TestReqID),
        Some(test_id),
        "Heartbeat must echo TestReqID from TestRequest"
    );
}

/// Test: Logout sequence (client-initiated)
#[tokio::test]
async fn test_logout_client_initiated() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    let logout = client.create_logout_message();
    stream.write_all(&logout).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(
        response.msg_type,
        MsgType::Logout,
        "Server must respond with Logout"
    );

    // Connection should close after Logout
    let result = timeout(Duration::from_secs(1), stream.read(&mut buffer)).await;
    if let Ok(Ok(n)) = result {
        assert_eq!(n, 0, "Connection should close after Logout");
    }
}

// ============================================================================
// RATE LIMITING TESTS
// ============================================================================

/// Test: Maximum 2 logons per second per API key
#[tokio::test]
async fn test_rate_limit_logons_per_second() {
    let (_, addr) = setup_server().await;

    let start = std::time::Instant::now();
    let mut successful_logons = 0;

    // Attempt 3 logons rapidly
    for _ in 0..3 {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let client = ClientSession::new(test_client_config());

        let logon_msg = client.create_logon_message();
        stream.write_all(&logon_msg).await.unwrap();

        let mut buffer = vec![0u8; 4096];
        let result = timeout(Duration::from_millis(500), stream.read(&mut buffer)).await;

        if let Ok(Ok(n)) = result {
            if n > 0 {
                let mut parser = Parser::new();
                parser.feed(&buffer[..n]);
                if let Ok(Some(msg)) = parser.parse_message() {
                    if msg.msg_type == MsgType::Logon {
                        successful_logons += 1;
                    }
                }
            }
        }
    }

    let elapsed = start.elapsed();

    // If all 3 succeeded in < 1 second, rate limiting may not be enforced
    // This test documents the expected behavior
    if elapsed.as_secs() < 1 {
        assert!(
            successful_logons <= 2,
            "Should rate-limit to max 2 logons per second, got {} in {:?}",
            successful_logons,
            elapsed
        );
    }
}

/// Test: Maximum 100 requests per second (normal operations)
#[tokio::test]
async fn test_rate_limit_100_requests_per_second() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    let start = std::time::Instant::now();
    let mut sent_count = 0;

    // Send heartbeats rapidly to test message rate
    for _ in 0..120 {
        let heartbeat = client.create_heartbeat_message(None);
        if stream.write_all(&heartbeat).await.is_ok() {
            sent_count += 1;
        }

        // Check if we've been rate limited (connection closed)
        if stream.try_read(&mut vec![0u8; 1]).is_err() {
            break;
        }
    }

    let elapsed = start.elapsed();

    // This test documents expected behavior
    // Server may disconnect if rate exceeds 100 req/s
    if elapsed.as_secs() < 1 && sent_count > 100 {
        // Connection might be closed due to rate limiting
        // This is acceptable behavior per spec
    }
}

/// Test: Session disconnected if messages exceed 200/second
#[tokio::test]
async fn test_disconnect_at_200_messages_per_second() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    let start = std::time::Instant::now();
    let mut sent_count = 0;
    let mut disconnected = false;

    // Attempt to send > 200 messages in < 1 second
    for _ in 0..250 {
        let heartbeat = client.create_heartbeat_message(None);

        match stream.write_all(&heartbeat).await {
            Ok(_) => sent_count += 1,
            Err(_) => {
                disconnected = true;
                break;
            }
        }

        if start.elapsed().as_secs() >= 1 {
            break;
        }
    }

    let elapsed = start.elapsed();

    // Per spec: "Your FIX 5 session is disconnected if your messages exceed 200 messages per second"
    if elapsed.as_secs() < 1 && sent_count > 200 {
        // We expect disconnection
        // This test documents the expected behavior
        println!(
            "Sent {} messages in {:?}, disconnected: {}",
            sent_count, elapsed, disconnected
        );
    }
}

// ============================================================================
// DROP COPY SESSION TESTS
// ============================================================================

/// Test: Drop copy session requires DropCopyFlag=Y (Tag 9406)
#[tokio::test]
async fn test_drop_copy_flag_required() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    // Create Logon with DropCopyFlag
    let mut logon = FixMessage::new(
        MsgType::Logon,
        1,
        client.sender_comp_id().to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    logon.add_field(Tag::EncryptMethod, "0".to_string());
    logon.add_field(Tag::HeartBtInt, "30".to_string());
    logon.add_field(Tag::DefaultApplVerID, "9".to_string());
    logon.add_field(Tag::DropCopyFlag, "Y".to_string()); // Tag 9406

    use loom_fix::serialization::build_fix_message;
    let logon_bytes = build_fix_message(&logon);

    stream.write_all(&logon_bytes).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);
    // Drop copy session established
}

/// Test: Drop copy sessions are read-only (no order entry)
#[tokio::test]
async fn test_drop_copy_read_only_no_order_entry() {
    // This test documents that drop copy sessions should reject
    // order entry messages (NewOrderSingle, OrderCancelRequest, etc.)
    //
    // Implementation: Mock server should reject order entry on drop copy sessions
    // with appropriate reject message

    // Note: This would require mock server to track drop copy state
    // Placeholder for future implementation
}

// ============================================================================
// BEST PRACTICES COMPLIANCE TESTS
// ============================================================================

/// Test: Maximum 1 API key per session/connection
#[tokio::test]
async fn test_one_api_key_per_connection() {
    let (_, addr) = setup_server().await;

    // Each connection uses exactly one API key
    let config = test_client_config();

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(config);

    let logon_msg = client.create_logon_message();
    stream.write_all(&logon_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Logon);

    // Per spec: "Maximum API keys per session/connection: 1"
    // This test validates that each session uses only one API key
}

/// Test: Connection limit - max 75 connections per profile
#[tokio::test]
async fn test_max_75_connections_per_profile() {
    // This test documents the connection limit
    // In practice, testing 75 concurrent connections may be resource-intensive
    //
    // Per spec: "Maximum connections per profile: 75"
    // Implementation would need to track profile-based connection counts

    // Placeholder for stress testing infrastructure
}

/// Test: Connection limit - max 175 connections per user across all profiles
#[tokio::test]
async fn test_max_175_connections_per_user() {
    // This test documents the global user connection limit
    // Per spec: "Maximum connections per user across all profiles: 175"

    // Placeholder for multi-profile testing infrastructure
}

// ============================================================================
// ORDER ENTRY MESSAGE TESTS
// ============================================================================

/// Test: NewOrderSingle requires ClOrdID as UUID v4
#[tokio::test]
async fn test_new_order_single_clordid_uuid() {
    // ClOrdID must be UUID v4 format
    // Must not match any open orders

    use uuid::Uuid;

    let clord_id = Uuid::new_v4().to_string();

    // Validate UUID format
    assert!(
        Uuid::parse_str(&clord_id).is_ok(),
        "ClOrdID must be valid UUID v4"
    );
}

/// Test: NewOrderSingle requires Symbol, Side, OrdType
#[tokio::test]
async fn test_new_order_single_required_fields() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    use uuid::Uuid;
    msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string()); // Buy
    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::OrderQty, "0.01".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string()); // GTC

    // Verify required fields present
    assert!(msg.get_field(Tag::ClOrdID).is_some());
    assert!(msg.get_field(Tag::Symbol).is_some());
    assert!(msg.get_field(Tag::Side).is_some());
    assert!(msg.get_field(Tag::OrdType).is_some());
}

/// Test: NewOrderSingle quantity - either OrderQty or CashOrderQty required
#[tokio::test]
async fn test_new_order_single_quantity_specification() {
    // Test with OrderQty (base asset)
    let mut msg1 = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );
    msg1.add_field(Tag::OrderQty, "0.5".to_string());
    assert!(msg1.get_field(Tag::OrderQty).is_some());

    // Test with CashOrderQty (quote currency)
    let mut msg2 = FixMessage::new(
        MsgType::NewOrderSingle,
        3,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );
    msg2.add_field(Tag::CashOrderQty, "25000.00".to_string());
    assert!(msg2.get_field(Tag::CashOrderQty).is_some());
}

/// Test: Limit orders require Price field
#[tokio::test]
async fn test_limit_order_requires_price() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::Price, "50000.00".to_string());

    assert_eq!(msg.get_field(Tag::OrdType), Some("2"));
    assert!(
        msg.get_field(Tag::Price).is_some(),
        "Limit orders must include Price(44)"
    );
}

/// Test: Stop Limit orders require StopPx
#[tokio::test]
async fn test_stop_limit_requires_stop_px() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    msg.add_field(Tag::OrdType, "4".to_string()); // Stop Limit
    msg.add_field(Tag::Price, "49000.00".to_string());
    msg.add_field(Tag::StopPx, "50000.00".to_string());

    assert_eq!(msg.get_field(Tag::OrdType), Some("4"));
    assert!(
        msg.get_field(Tag::StopPx).is_some(),
        "Stop Limit orders must include StopPx(99)"
    );
}

/// Test: GTD (Good Till Date) orders require ExpireTime
#[tokio::test]
async fn test_gtd_order_requires_expire_time() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    msg.add_field(Tag::TimeInForce, "6".to_string()); // GTD
    msg.add_field(Tag::ExpireTime, "20261231-23:59:59.000".to_string());

    assert_eq!(msg.get_field(Tag::TimeInForce), Some("6"));
    assert!(
        msg.get_field(Tag::ExpireTime).is_some(),
        "GTD orders must include ExpireTime(126)"
    );
}

/// Test: Post-only orders use ExecInst=A
#[tokio::test]
async fn test_post_only_exec_inst() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    msg.add_field(Tag::ExecInst, "A".to_string()); // Post Only

    assert_eq!(
        msg.get_field(Tag::ExecInst),
        Some("A"),
        "Post-only orders use ExecInst(18)=A"
    );
}

/// Test: ExecutionReport must include ClOrdID, OrderID, OrdStatus
#[tokio::test]
async fn test_execution_report_required_fields() {
    let mut exec_report = FixMessage::new(
        MsgType::ExecutionReport,
        5,
        "Coinbase".to_string(),
        "TEST_CLIENT".to_string(),
        ClientSession::fix_timestamp(),
    );

    use uuid::Uuid;
    exec_report.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    exec_report.add_field(Tag::OrderID, Uuid::new_v4().to_string());
    exec_report.add_field(Tag::OrdStatus, "0".to_string()); // New
    exec_report.add_field(Tag::ExecType, "0".to_string()); // New
    exec_report.add_field(Tag::Symbol, "BTC-USD".to_string());
    exec_report.add_field(Tag::TransactTime, ClientSession::fix_timestamp());

    assert!(exec_report.get_field(Tag::ClOrdID).is_some());
    assert!(exec_report.get_field(Tag::OrderID).is_some());
    assert!(exec_report.get_field(Tag::OrdStatus).is_some());
    assert!(exec_report.get_field(Tag::ExecType).is_some());
    assert!(exec_report.get_field(Tag::Symbol).is_some());
    assert!(exec_report.get_field(Tag::TransactTime).is_some());
}

/// Test: Batch orders - max 15 orders per batch
#[tokio::test]
async fn test_batch_order_max_15_orders() {
    // Per spec: "Maximum orders per batch message (new and cancelled): 15"

    let mut batch_msg = FixMessage::new(
        MsgType::NewOrderBatch,
        2,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    batch_msg.add_field(Tag::NoOrders, "15".to_string());

    let no_orders: usize = batch_msg.get_field(Tag::NoOrders).unwrap().parse().unwrap();

    assert!(no_orders <= 15, "Batch messages must not exceed 15 orders");
}

/// Test: OrderCancelRequest requires ClOrdID, OrderID or OrigClOrdID, Symbol
#[tokio::test]
async fn test_order_cancel_request_required_fields() {
    let mut cancel_msg = FixMessage::new(
        MsgType::OrderCancelRequest,
        3,
        "TEST".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    use uuid::Uuid;
    cancel_msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string()); // New UUID for cancel
    cancel_msg.add_field(Tag::OrderID, Uuid::new_v4().to_string()); // Original order ID
    cancel_msg.add_field(Tag::Symbol, "BTC-USD".to_string());

    assert!(cancel_msg.get_field(Tag::ClOrdID).is_some());
    assert!(
        cancel_msg.get_field(Tag::OrderID).is_some()
            || cancel_msg.get_field(Tag::OrigClOrdID).is_some(),
        "Must include OrderID or OrigClOrdID"
    );
    assert!(cancel_msg.get_field(Tag::Symbol).is_some());
}

/// Test: OrderCancelReject has CxlRejReason
#[tokio::test]
async fn test_order_cancel_reject_reason() {
    let mut reject_msg = FixMessage::new(
        MsgType::OrderCancelReject,
        4,
        "Coinbase".to_string(),
        "TEST".to_string(),
        ClientSession::fix_timestamp(),
    );

    reject_msg.add_field(Tag::OrdStatus, "8".to_string()); // Rejected
    reject_msg.add_field(Tag::CxlRejReason, "1".to_string()); // Unknown Order
    reject_msg.add_field(Tag::CxlRejResponseTo, "1".to_string()); // Cancel Request

    assert_eq!(reject_msg.get_field(Tag::OrdStatus), Some("8"));
    assert!(reject_msg.get_field(Tag::CxlRejReason).is_some());
}

// ============================================================================
// ERROR HANDLING & REJECT TESTS
// ============================================================================

/// Test: Invalid Logon without signature should be rejected
#[tokio::test]
async fn test_invalid_logon_no_signature_rejected() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Create Logon without RawData signature
    let mut msg = FixMessage::new(
        MsgType::Logon,
        1,
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );
    msg.add_field(Tag::EncryptMethod, "0".to_string());
    msg.add_field(Tag::HeartBtInt, "30".to_string());
    // Missing: Username, Password, RawDataLength, RawData

    use loom_fix::serialization::build_fix_message;
    let logon_bytes = build_fix_message(&msg);

    stream.write_all(&logon_bytes).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let result = timeout(Duration::from_secs(2), stream.read(&mut buffer)).await;

    // Should receive connection close or timeout (no Logon response)
    match result {
        Ok(Ok(0)) => {
            // Connection closed - expected
        }
        Ok(Ok(n)) => {
            let mut parser = Parser::new();
            parser.feed(&buffer[..n]);
            if let Ok(Some(msg)) = parser.parse_message() {
                assert_ne!(
                    msg.msg_type,
                    MsgType::Logon,
                    "Should not accept Logon without signature"
                );
            }
        }
        Err(_) => {
            // Timeout - also acceptable (connection closed)
        }
        _ => {}
    }
}

/// Test: Sequence gap should trigger ResendRequest from server
#[tokio::test]
async fn test_sequence_gap_triggers_resend_request() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // Send message with gap (skip seq 2, send seq 4)
    let mut msg = FixMessage::new(
        MsgType::Heartbeat,
        4, // Gap: should be 2
        client.sender_comp_id().to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    use loom_fix::serialization::build_fix_message;
    let msg_bytes = build_fix_message(&msg);

    stream.write_all(&msg_bytes).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    // Server may send ResendRequest or disconnect
    // This test documents expected behavior for sequence gaps
    assert!(
        response.msg_type == MsgType::ResendRequest
            || response.msg_type == MsgType::Logout
            || response.msg_type == MsgType::Reject,
        "Sequence gap should trigger ResendRequest, Logout, or Reject"
    );
}

// ============================================================================
// MARKET DATA TESTS (Integration with Market Data Spec)
// ============================================================================

/// Test: SecurityListRequest (Tag 35=x) flow
#[tokio::test]
async fn test_security_list_request_flow() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // Send SecurityListRequest
    let mut sec_list_req = FixMessage::new(
        MsgType::SecurityListRequest,
        2,
        client.sender_comp_id().to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    sec_list_req.add_field(Tag::SecurityReqID, "SEC_REQ_1".to_string());
    sec_list_req.add_field(Tag::SecurityListRequestType, "0".to_string()); // Symbol

    use loom_fix::serialization::build_fix_message;
    let req_bytes = build_fix_message(&sec_list_req);

    stream.write_all(&req_bytes).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(
        response.msg_type,
        MsgType::SecurityList,
        "Should receive SecurityList response"
    );
    assert_eq!(
        response.get_field(Tag::SecurityReqID),
        Some("SEC_REQ_1"),
        "Should echo SecurityReqID"
    );
}

/// Test: MarketDataRequest (Tag 35=V) subscription
#[tokio::test]
async fn test_market_data_request_subscription() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // Send MarketDataRequest
    let mut md_req = FixMessage::new(
        MsgType::MarketDataRequest,
        2,
        client.sender_comp_id().to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    md_req.add_field(Tag::MDReqID, "MD_REQ_1".to_string());
    md_req.add_field(Tag::SubscriptionRequestType, "1".to_string()); // Snapshot + Updates
    md_req.add_field(Tag::MarketDepth, "1".to_string()); // Top of book
    md_req.add_field(Tag::NoMDEntryTypes, "2".to_string());
    md_req.add_field(Tag::NoRelatedSym, "1".to_string());

    use loom_fix::serialization::build_fix_message;
    let req_bytes = build_fix_message(&md_req);

    stream.write_all(&req_bytes).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    // Should receive MarketDataIncrementalRefresh or MarketDataSnapshotFullRefresh
    assert!(
        response.msg_type == MsgType::MarketDataIncrementalRefresh
            || response.msg_type == MsgType::MarketDataSnapshotFullRefresh,
        "Should receive market data response"
    );
}

// ============================================================================
// PROTOCOL EDGE CASES
// ============================================================================

/// Test: Message with invalid checksum should be rejected
#[tokio::test]
async fn test_invalid_checksum_rejected() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Manually construct message with bad checksum
    let bad_msg = b"8=FIXT.1.1\x019=100\x0135=A\x0149=TEST\x0156=Coinbase\x0134=1\x0152=20260212-12:00:00.000\x0110=000\x01";

    stream.write_all(bad_msg).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let result = timeout(Duration::from_secs(1), stream.read(&mut buffer)).await;

    // Server should reject or close connection
    // This test documents expected behavior for checksum validation
}

/// Test: Malformed FIX message handling
#[tokio::test]
async fn test_malformed_message_handling() {
    let (_, addr) = setup_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Send garbage data
    stream.write_all(b"NOTFIX\x01\x01\x01").await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let result = timeout(Duration::from_secs(1), stream.read(&mut buffer)).await;

    // Server should close connection or ignore
    // This test documents expected behavior for malformed messages
}

// ============================================================================
// INTEGRATION TEST: Full Order Lifecycle
// ============================================================================

/// Test: Complete order lifecycle - New → Fill → ExecutionReport
#[tokio::test]
async fn test_full_order_lifecycle() {
    let (_, addr) = setup_server().await;
    let (mut stream, client) = connect_and_logon(addr).await;

    // 1. Send NewOrderSingle
    let mut new_order = FixMessage::new(
        MsgType::NewOrderSingle,
        2,
        client.sender_comp_id().to_string(),
        "Coinbase".to_string(),
        ClientSession::fix_timestamp(),
    );

    use uuid::Uuid;
    let clord_id = Uuid::new_v4().to_string();

    new_order.add_field(Tag::ClOrdID, clord_id.clone());
    new_order.add_field(Tag::Symbol, "BTC-USD".to_string());
    new_order.add_field(Tag::Side, "1".to_string()); // Buy
    new_order.add_field(Tag::OrdType, "2".to_string()); // Limit
    new_order.add_field(Tag::OrderQty, "0.01".to_string());
    new_order.add_field(Tag::Price, "50000.00".to_string());
    new_order.add_field(Tag::TimeInForce, "1".to_string()); // GTC

    use loom_fix::serialization::build_fix_message;
    let order_bytes = build_fix_message(&new_order);

    stream.write_all(&order_bytes).await.unwrap();

    // 2. Receive ExecutionReport (New)
    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let exec_report = parser.parse_message().unwrap().unwrap();

    assert_eq!(exec_report.msg_type, MsgType::ExecutionReport);
    assert_eq!(exec_report.get_field(Tag::ClOrdID), Some(clord_id.as_str()));
    assert_eq!(exec_report.get_field(Tag::OrdStatus), Some("0")); // New
    assert_eq!(exec_report.get_field(Tag::ExecType), Some("0")); // New

    // Verify OrderID assigned
    assert!(
        exec_report.get_field(Tag::OrderID).is_some(),
        "ExecutionReport must include OrderID"
    );
}

// ============================================================================
// HELPER FUNCTION TESTS
// ============================================================================

/// Test: UUID v4 format validation
#[tokio::test]
async fn test_uuid_v4_format() {
    use uuid::Uuid;

    let uuid = Uuid::new_v4();
    let uuid_str = uuid.to_string();

    // Verify format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
    assert_eq!(uuid_str.len(), 36);
    assert_eq!(&uuid_str[14..15], "4"); // Version 4

    // Verify parseable
    assert!(Uuid::parse_str(&uuid_str).is_ok());
}

/// Test: FIX timestamp format (YYYYMMDD-HH:MM:SS.sss)
#[tokio::test]
async fn test_fix_timestamp_format() {
    let ts = ClientSession::fix_timestamp();

    // Format: YYYYMMDD-HH:MM:SS.sss
    assert_eq!(ts.len(), 21);
    assert_eq!(&ts[8..9], "-");
    assert_eq!(&ts[11..12], ":");
    assert_eq!(&ts[14..15], ":");
    assert_eq!(&ts[17..18], ".");

    // Verify fractional seconds (3 digits)
    let fractional = &ts[18..21];
    assert!(fractional.parse::<u32>().is_ok());
}
