// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Integration tests for FIX client session
//!
//! These tests verify the session layer with a simple mock server

use loom_fix::message::MsgType;
use loom_fix::session::client::{ClientSession, SessionConfig, SessionState};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

fn test_config() -> SessionConfig {
    SessionConfig {
        sender_comp_id: "TEST_API_KEY".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_passphrase".to_string(),
        heartbeat_interval: 30,
    }
}

#[tokio::test]
async fn test_session_state_transitions() {
    let mut session = ClientSession::new(test_config());

    // Initial state
    assert_eq!(session.state(), SessionState::Disconnected);

    // Transition to LoggingIn
    session.set_state(SessionState::LoggingIn);
    assert_eq!(session.state(), SessionState::LoggingIn);

    // Transition to LoggedIn
    session.set_state(SessionState::LoggedIn);
    assert_eq!(session.state(), SessionState::LoggedIn);

    // Transition to LoggingOut
    session.set_state(SessionState::LoggingOut);
    assert_eq!(session.state(), SessionState::LoggingOut);

    // Back to Disconnected
    session.set_state(SessionState::Disconnected);
    assert_eq!(session.state(), SessionState::Disconnected);
}

#[tokio::test]
async fn test_logon_message_structure() {
    let session = ClientSession::new(test_config());
    let logon = session.create_logon_message();

    // Verify message starts correctly
    assert!(logon.starts_with(b"8=FIXT.1.1\x01"));

    // Verify it's a valid message (can be parsed)
    let mut parser = loom_fix::Parser::new();
    parser.feed(&logon);
    let parsed = parser.parse_message().unwrap().unwrap();

    // Verify structure
    assert_eq!(parsed.msg_type, MsgType::Logon);
    assert_eq!(parsed.msg_seq_num, 1);
    assert!(parsed
        .get_field(loom_fix::message::Tag::HeartBtInt)
        .is_some());
    assert!(parsed.get_field(loom_fix::message::Tag::RawData).is_some());
}

#[tokio::test]
async fn test_sequence_number_increment() {
    let session = ClientSession::new(test_config());

    // Create multiple messages - sequence should increment
    let msg1 = session.create_heartbeat_message(None);
    let msg2 = session.create_heartbeat_message(None);
    let msg3 = session.create_heartbeat_message(None);

    // Parse and verify sequence numbers
    let mut parser = loom_fix::Parser::new();

    parser.feed(&msg1);
    let parsed1 = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed1.msg_seq_num, 1);

    parser.feed(&msg2);
    let parsed2 = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed2.msg_seq_num, 2);

    parser.feed(&msg3);
    let parsed3 = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed3.msg_seq_num, 3);
}

#[tokio::test]
async fn test_resend_request_creation() {
    let session = ClientSession::new(test_config());
    let resend = session.create_resend_request(5, 10);

    let mut parser = loom_fix::Parser::new();
    parser.feed(&resend);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::ResendRequest);
    assert_eq!(
        parsed.get_field(loom_fix::message::Tag::BeginSeqNo),
        Some("5")
    );
    assert_eq!(
        parsed.get_field(loom_fix::message::Tag::EndSeqNo),
        Some("10")
    );
}

#[tokio::test]
async fn test_heartbeat_idle_detection() {
    let mut session = ClientSession::new(test_config());

    // Initially, not logged in, should not send heartbeat
    assert!(!session.should_send_heartbeat());

    // Set to logged in
    session.set_state(SessionState::LoggedIn);

    // Just sent message, should not need heartbeat
    session.update_last_send_time();
    assert!(!session.should_send_heartbeat());

    // Wait for idle threshold (would need tokio::time::advance in real test)
    // For now, just verify the logic exists
}

#[tokio::test]
async fn test_gap_detection_and_recovery() {
    let session = ClientSession::new(test_config());

    // Expect sequence 1
    assert!(session.check_sequence_number(1));
    assert!(!session.check_sequence_number(2)); // Gap!
    assert!(!session.check_sequence_number(5)); // Bigger gap!

    // Process message 1
    session.increment_seq_num_in();

    // Now expect 2
    assert!(session.check_sequence_number(2));
    assert!(!session.check_sequence_number(3));

    // If we detect a gap (received 5 instead of 2), we should send ResendRequest
    let resend = session.create_resend_request(2, 0); // 0 means "to infinity"

    let mut parser = loom_fix::Parser::new();
    parser.feed(&resend);
    let parsed = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed.msg_type, MsgType::ResendRequest);
}

#[tokio::test]
async fn test_message_parsing_and_feeding() {
    let mut session = ClientSession::new(test_config());

    // Create a message
    let heartbeat = session.create_heartbeat_message(None);

    // Feed it back (simulating receiving own message for test)
    session.feed_data(&heartbeat);

    // Should be able to parse it
    let parsed = session.parse_next_message().unwrap();
    assert!(parsed.is_some());

    let msg = parsed.unwrap();
    assert_eq!(msg.msg_type, MsgType::Heartbeat);
}

#[tokio::test]
async fn test_sequence_reset() {
    let session = ClientSession::new(test_config());

    // Increment sequence numbers
    session.create_heartbeat_message(None);
    session.create_heartbeat_message(None);
    session.create_heartbeat_message(None);

    assert_eq!(session.current_seq_num_out(), 4); // Started at 1, created 3 messages

    // Reset
    session.reset_sequence_numbers();

    assert_eq!(session.current_seq_num_out(), 1);
    assert_eq!(session.expected_seq_num_in(), 1);
}

// Simple mock server for integration testing
async fn run_mock_server(listener: TcpListener) -> Result<(), Box<dyn std::error::Error>> {
    let (mut socket, _) = listener.accept().await?;

    let mut buffer = vec![0u8; 4096];

    // Read Logon message
    let n = socket.read(&mut buffer).await?;
    if n == 0 {
        return Ok(());
    }

    // Simple echo response (for basic test)
    // In reality, would parse and respond with proper Logon response
    socket.write_all(b"8=FIXT.1.1\x019=50\x0135=A\x0134=1\x0149=Coinbase\x0156=TEST\x0152=20260212-12:00:00.000\x0110=000\x01").await?;

    Ok(())
}

#[tokio::test]
async fn test_basic_connection_flow() {
    // This is a placeholder for full integration testing
    // The test-harness-engineer will build comprehensive mock server

    let session = ClientSession::new(test_config());

    // For now, just verify we can create all message types
    let _logon = session.create_logon_message();
    let _heartbeat = session.create_heartbeat_message(None);
    let _test_req = session.create_test_request_message("TEST123");
    let _logout = session.create_logout_message();

    // All messages created successfully
    assert!(true);
}
