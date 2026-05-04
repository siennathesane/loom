// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use loom_fix::message::{FixMessage, MsgType};
use loom_fix::parser::Parser;
use loom_fix::session::client::{ClientSession, SessionConfig, SessionState};

#[test]
fn test_session_creation() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);
    assert_eq!(session.state(), SessionState::Disconnected);
    assert_eq!(session.current_seq_num_out(), 1);
    assert_eq!(session.expected_seq_num_in(), 1);
}

#[test]
fn test_logon_message_creation() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);
    let logon_msg = session.create_logon_message();

    // Verify it's valid FIX format (starts with 8=FIXT.1.1)
    assert!(logon_msg.starts_with(b"8=FIXT.1.1"));

    // Message should contain sender and target comp IDs
    let msg_str = String::from_utf8_lossy(&logon_msg);
    assert!(msg_str.contains("TEST_SENDER"));
    assert!(msg_str.contains("TEST_TARGET"));
}

#[test]
fn test_heartbeat_message_creation() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);
    let hb_msg = session.create_heartbeat_message(None);

    assert!(hb_msg.starts_with(b"8=FIXT.1.1"));
}

#[test]
fn test_heartbeat_with_test_req_id() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);
    let test_req_id = "TEST123";
    let hb_msg = session.create_heartbeat_message(Some(test_req_id));

    let msg_str = String::from_utf8_lossy(&hb_msg);
    assert!(msg_str.contains(test_req_id));
}

#[test]
fn test_logout_message_creation() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);
    let logout_msg = session.create_logout_message();

    assert!(logout_msg.starts_with(b"8=FIXT.1.1"));
}

#[test]
fn test_resend_request_creation() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);
    let resend_msg = session.create_resend_request(10, 20);

    assert!(resend_msg.starts_with(b"8=FIXT.1.1"));
}

#[test]
fn test_sequence_number_management() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);

    // Initial sequence numbers
    assert_eq!(session.current_seq_num_out(), 1);
    assert_eq!(session.expected_seq_num_in(), 1);

    // Create message (increments outgoing)
    let _logon = session.create_logon_message();
    assert_eq!(session.current_seq_num_out(), 2);

    // Increment incoming
    session.increment_seq_num_in();
    assert_eq!(session.expected_seq_num_in(), 2);
}

#[test]
fn test_sequence_number_check() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);

    // Should expect sequence 1
    assert!(session.check_sequence_number(1));
    assert!(!session.check_sequence_number(2));

    // After increment, should expect 2
    session.increment_seq_num_in();
    assert!(session.check_sequence_number(2));
    assert!(!session.check_sequence_number(1));
}

#[tokio::test]
async fn test_session_state_transitions() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let mut session = ClientSession::new(config);

    assert_eq!(session.state(), SessionState::Disconnected);

    session.set_state(SessionState::LoggingIn);
    assert_eq!(session.state(), SessionState::LoggingIn);

    session.set_state(SessionState::LoggedIn);
    assert_eq!(session.state(), SessionState::LoggedIn);

    session.set_state(SessionState::LoggingOut);
    assert_eq!(session.state(), SessionState::LoggingOut);
}

#[test]
fn test_parser_integration() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let mut session = ClientSession::new(config);

    // Create a message
    let hb_msg = session.create_heartbeat_message(None);

    // Feed it to another session's parser
    session.feed_data(&hb_msg);

    // Should be able to parse it
    let parsed = session.parse_next_message();
    assert!(parsed.is_ok());
}

#[test]
fn test_sequence_number_reset() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);

    // Increment sequences
    let _logon = session.create_logon_message();
    session.increment_seq_num_in();

    assert!(session.current_seq_num_out() > 1);
    assert!(session.expected_seq_num_in() > 1);

    // Reset
    session.reset_sequence_numbers();

    assert_eq!(session.current_seq_num_out(), 1);
    assert_eq!(session.expected_seq_num_in(), 1);
}

#[tokio::test]
async fn test_heartbeat_timing() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 10, // 10 seconds (0.75 * 10 = 7.5 => 7 seconds)
    };

    let mut session = ClientSession::new(config);
    session.set_state(SessionState::LoggedIn);

    // Initially should not need heartbeat
    assert!(!session.should_send_heartbeat());

    // Update last send time (simulating we just sent)
    session.update_last_send_time();

    // Still shouldn't need heartbeat immediately
    assert!(!session.should_send_heartbeat());
}

#[test]
fn test_multiple_message_creation() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let session = ClientSession::new(config);

    // Create multiple messages
    let _logon = session.create_logon_message();
    let _hb1 = session.create_heartbeat_message(None);
    let _hb2 = session.create_heartbeat_message(Some("TEST"));
    let _logout = session.create_logout_message();

    // Sequence should be incremented for each
    assert_eq!(session.current_seq_num_out(), 5);
}

#[test]
fn test_config_cloning() {
    let config = SessionConfig {
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        api_secret: "test_secret".to_string(),
        api_passphrase: "test_pass".to_string(),
        heartbeat_interval: 30,
    };

    let config2 = config.clone();

    assert_eq!(config.sender_comp_id, config2.sender_comp_id);
    assert_eq!(config.target_comp_id, config2.target_comp_id);
    assert_eq!(config.heartbeat_interval, config2.heartbeat_interval);
}

#[test]
fn test_session_state_copy() {
    let state1 = SessionState::LoggedIn;
    let state2 = state1;

    assert_eq!(state1, state2);
}
