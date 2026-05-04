// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Protocol compliance tests for loom-fix implementation.
//!
//! These tests validate that OUR FIX implementation correctly implements
//! the Coinbase FIX 5.0 SP2 protocol specification by:
//!
//! 1. Message serialization produces spec-compliant output
//! 2. Parser correctly validates protocol constraints
//! 3. Authentication generates proper HMAC signatures
//! 4. Client session follows protocol rules
//!
//! These are UNIT tests for our implementation, not integration tests.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::{ParseError, Parser};
use loom_fix::serialization::{build_fix_message, calculate_checksum, format_checksum};
use loom_fix::session::auth::compute_logon_hmac;
use loom_fix::session::client::{ClientSession, SessionConfig};

// ============================================================================
// SERIALIZATION COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_message_begins_with_fixt_1_1() {
    let msg = FixMessage::new(
        MsgType::Logon,
        1,
        "TEST_API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    assert!(
        msg_str.starts_with("8=FIXT.1.1\x01"),
        "Message must start with BeginString=FIXT.1.1, got: {}",
        &msg_str[..20]
    );
}

#[test]
fn test_body_length_calculation_correct() {
    let msg = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    // Extract BodyLength field
    let body_length_start = msg_str.find("9=").expect("Missing BodyLength");
    let body_length_end = msg_str[body_length_start..]
        .find('\x01')
        .expect("Missing SOH after BodyLength");
    let body_length_str = &msg_str[body_length_start + 2..body_length_start + body_length_end];
    let declared_length: usize = body_length_str.parse().expect("Invalid BodyLength");

    // Calculate actual body length (from after BodyLength to before CheckSum)
    let body_start = msg_str.find('\x01').unwrap() + 1; // After BeginString
    let body_start = msg_str[body_start..].find('\x01').unwrap() + body_start + 1; // After BodyLength
    let checksum_pos = msg_str.rfind("10=").expect("Missing CheckSum");
    let actual_length = checksum_pos - body_start;

    assert_eq!(
        declared_length, actual_length,
        "BodyLength mismatch: declared {}, actual {}",
        declared_length, actual_length
    );
}

#[test]
fn test_checksum_calculation_correct() {
    let msg = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let serialized = build_fix_message(&msg);

    // Parse and verify checksum
    let mut parser = Parser::new();
    parser.feed(&serialized);

    let result = parser.parse_message();
    assert!(
        result.is_ok(),
        "Checksum validation failed: {:?}",
        result.err()
    );
    assert!(
        result.unwrap().is_some(),
        "Message should parse successfully"
    );
}

#[test]
fn test_checksum_format_three_digits() {
    // Test checksum formatter
    assert_eq!(format_checksum(0), [b'0', b'0', b'0']);
    assert_eq!(format_checksum(5), [b'0', b'0', b'5']);
    assert_eq!(format_checksum(42), [b'0', b'4', b'2']);
    assert_eq!(format_checksum(100), [b'1', b'0', b'0']);
    assert_eq!(format_checksum(255), [b'2', b'5', b'5']);

    // Verify in actual message
    let msg = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "S".to_string(),
        "T".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    let checksum_pos = msg_str.rfind("10=").expect("Missing CheckSum");
    let checksum_str = &msg_str[checksum_pos + 3..checksum_pos + 6];

    assert_eq!(
        checksum_str.len(),
        3,
        "CheckSum must be exactly 3 digits, got: {}",
        checksum_str
    );
    assert!(
        checksum_str.chars().all(|c| c.is_ascii_digit()),
        "CheckSum must be numeric, got: {}",
        checksum_str
    );
}

#[test]
fn test_sequence_number_no_leading_zeros() {
    for seq_num in [1, 2, 10, 99, 100, 999, 1000] {
        let msg = FixMessage::new(
            MsgType::Heartbeat,
            seq_num,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let serialized = build_fix_message(&msg);
        let msg_str = String::from_utf8_lossy(&serialized);

        let seq_pattern = format!("34={}\x01", seq_num);
        assert!(
            msg_str.contains(&seq_pattern),
            "MsgSeqNum must not have leading zeros: expected '34={}', in message: {}",
            seq_num,
            msg_str
        );
    }
}

#[test]
fn test_sending_time_fractional_seconds_format() {
    let msg = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:34:56.789".to_string(),
    );

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    // Extract SendingTime
    let sending_time_start = msg_str.find("52=").expect("Missing SendingTime");
    let sending_time_end = msg_str[sending_time_start..]
        .find('\x01')
        .expect("Missing SOH after SendingTime");
    let sending_time = &msg_str[sending_time_start + 3..sending_time_start + sending_time_end];

    // Validate format: YYYYMMDD-HH:MM:SS.sss
    assert_eq!(sending_time.len(), 21, "SendingTime must be 21 characters");
    assert_eq!(&sending_time[8..9], "-", "Missing dash after date");
    assert_eq!(&sending_time[11..12], ":", "Missing colon in time");
    assert_eq!(&sending_time[14..15], ":", "Missing colon in time");
    assert_eq!(&sending_time[17..18], ".", "Missing dot before millis");

    let fractional = &sending_time[18..21];
    assert_eq!(
        fractional.len(),
        3,
        "Fractional seconds must be exactly 3 digits"
    );
    assert!(
        fractional.chars().all(|c| c.is_ascii_digit()),
        "Fractional seconds must be numeric"
    );
}

#[test]
fn test_target_comp_id_coinbase() {
    let config = SessionConfig {
        sender_comp_id: "TEST_API_KEY".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 30,
    };

    let client = ClientSession::new(config);
    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    assert!(
        msg_str.contains("56=Coinbase\x01"),
        "TargetCompID must be 'Coinbase' for Coinbase FIX API"
    );
}

#[test]
fn test_logon_encrypt_method_zero() {
    let config = SessionConfig {
        sender_comp_id: "TEST".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 30,
    };

    let client = ClientSession::new(config);
    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    assert!(
        msg_str.contains("98=0\x01"),
        "EncryptMethod must be 0 (None) - SSL/TLS handles encryption"
    );
}

#[test]
fn test_logon_includes_authentication_fields() {
    let config = SessionConfig {
        sender_comp_id: "API_KEY".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "passphrase".to_string(),
        heartbeat_interval: 30,
    };

    let client = ClientSession::new(config);
    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    // Check for required authentication fields
    assert!(msg_str.contains("553="), "Logon must include Username(553)");
    assert!(msg_str.contains("554="), "Logon must include Password(554)");
    assert!(
        msg_str.contains("95="),
        "Logon must include RawDataLength(95)"
    );
    assert!(msg_str.contains("96="), "Logon must include RawData(96)");
}

// ============================================================================
// PARSER COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_parser_rejects_invalid_begin_string() {
    let invalid_msg = b"8=FIX.4.2\x019=50\x0135=0\x0149=S\x0156=T\x0134=1\x0152=20260212-12:00:00.000\x0110=000\x01";

    let mut parser = Parser::new();
    parser.feed(invalid_msg);

    let result = parser.parse_message();
    assert!(
        matches!(result, Err(ParseError::InvalidBeginString)),
        "Parser must reject non-FIXT.1.1 BeginString"
    );
}

#[test]
fn test_parser_rejects_checksum_mismatch() {
    let msg = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let mut serialized = build_fix_message(&msg);

    // Corrupt checksum - find "10=" and change the first digit
    let msg_str = String::from_utf8_lossy(&serialized);
    let checksum_pos = msg_str.find("10=").unwrap() + 3;
    serialized[checksum_pos] = if serialized[checksum_pos] == b'0' {
        b'1'
    } else {
        b'0'
    };

    let mut parser = Parser::new();
    parser.feed(&serialized);

    let result = parser.parse_message();
    assert!(
        matches!(result, Err(ParseError::CheckSumMismatch { .. })),
        "Parser must detect checksum mismatch, got: {:?}",
        result
    );
}

#[test]
fn test_parser_enforces_required_fields() {
    // Message missing MsgSeqNum
    let invalid_msg =
        b"8=FIXT.1.1\x019=30\x0135=0\x0149=S\x0156=T\x0152=20260212-12:00:00.000\x0110=150\x01";

    let mut parser = Parser::new();
    parser.feed(invalid_msg);

    let result = parser.parse_message();
    assert!(
        result.is_err(),
        "Parser must reject message missing required fields"
    );
}

#[test]
fn test_parser_handles_message_too_large() {
    // Create a message with declared BodyLength > MAX_MESSAGE_SIZE (10MB)
    let oversized_msg =
        b"8=FIXT.1.1\x019=99999999\x0135=0\x0149=S\x0156=T\x0134=1\x0152=20260212-12:00:00.000\x0110=000\x01";

    let mut parser = Parser::new();
    parser.feed(oversized_msg);

    let result = parser.parse_message();
    assert!(
        matches!(result, Err(ParseError::MessageTooLarge(_))),
        "Parser must reject messages exceeding size limit"
    );
}

#[test]
fn test_parser_handles_incomplete_messages() {
    let msg = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let serialized = build_fix_message(&msg);

    // Feed only part of the message
    let mut parser = Parser::new();
    parser.feed(&serialized[..serialized.len() / 2]);

    let result = parser.parse_message();
    assert!(
        result.is_ok() && result.unwrap().is_none(),
        "Parser should indicate incomplete message, not error"
    );

    // Feed the rest
    parser.feed(&serialized[serialized.len() / 2..]);

    let result = parser.parse_message();
    assert!(
        result.is_ok() && result.unwrap().is_some(),
        "Parser should successfully parse complete message"
    );
}

#[test]
fn test_parser_handles_multiple_messages_in_stream() {
    let msg1 = FixMessage::new(
        MsgType::Heartbeat,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    let msg2 = FixMessage::new(
        MsgType::TestRequest,
        2,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:01.000".to_string(),
    );

    let serialized1 = build_fix_message(&msg1);
    let serialized2 = build_fix_message(&msg2);

    let mut combined = serialized1;
    combined.extend_from_slice(&serialized2);

    let mut parser = Parser::new();
    parser.feed(&combined);

    // Parse first message
    let parsed1 = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed1.msg_type, MsgType::Heartbeat);
    assert_eq!(parsed1.msg_seq_num, 1);

    // Parse second message
    let parsed2 = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed2.msg_type, MsgType::TestRequest);
    assert_eq!(parsed2.msg_seq_num, 2);

    // No more messages
    assert!(parser.parse_message().unwrap().is_none());
}

// ============================================================================
// AUTHENTICATION COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_hmac_signature_is_deterministic() {
    let secret = "dGVzdF9zZWNyZXQ="; // base64("test_secret")
    let sender = "API_KEY";
    let target = "Coinbase";
    let msg_type = "A";
    let seq_num = 1;
    let sending_time = "20260212-12:00:00.000";
    let password = "passphrase";

    let hmac1 = compute_logon_hmac(
        secret,
        sender,
        target,
        msg_type,
        seq_num,
        sending_time,
        password,
    );

    let hmac2 = compute_logon_hmac(
        secret,
        sender,
        target,
        msg_type,
        seq_num,
        sending_time,
        password,
    );

    assert_eq!(hmac1, hmac2, "HMAC computation must be deterministic");
}

#[test]
fn test_hmac_signature_is_base64() {
    let hmac = compute_logon_hmac(
        "c2VjcmV0", // base64("secret")
        "SENDER",
        "TARGET",
        "A",
        1,
        "20260212-12:00:00.000",
        "pass",
    );

    use base64::{engine::general_purpose, Engine};
    let decoded = general_purpose::STANDARD.decode(&hmac);
    assert!(decoded.is_ok(), "HMAC must be valid base64");
    assert_eq!(decoded.unwrap().len(), 32, "HMAC-SHA256 produces 32 bytes");
}

#[test]
fn test_hmac_changes_with_parameters() {
    let base_hmac = compute_logon_hmac(
        "c2VjcmV0", // base64("secret")
        "SENDER",
        "TARGET",
        "A",
        1,
        "20260212-12:00:00.000",
        "pass",
    );

    // Change secret
    let hmac_diff_secret = compute_logon_hmac(
        "ZGlmZmVyZW50X3NlY3JldA==", // base64("different_secret")
        "SENDER",
        "TARGET",
        "A",
        1,
        "20260212-12:00:00.000",
        "pass",
    );
    assert_ne!(base_hmac, hmac_diff_secret, "HMAC must change with secret");

    // Change sequence number
    let hmac_diff_seq = compute_logon_hmac(
        "c2VjcmV0", // base64("secret")
        "SENDER",
        "TARGET",
        "A",
        2, // Changed
        "20260212-12:00:00.000",
        "pass",
    );
    assert_ne!(base_hmac, hmac_diff_seq, "HMAC must change with seq num");

    // Change sending time
    let hmac_diff_time = compute_logon_hmac(
        "c2VjcmV0", // base64("secret")
        "SENDER",
        "TARGET",
        "A",
        1,
        "20260212-12:00:01.000", // Changed
        "pass",
    );
    assert_ne!(
        base_hmac, hmac_diff_time,
        "HMAC must change with sending time"
    );
}

#[test]
fn test_hmac_signature_format_order() {
    // Per Coinbase spec: SendingTime + MsgType + MsgSeqNum + SenderCompID + TargetCompID + Passphrase
    let correct_order = compute_logon_hmac(
        "c2VjcmV0", // base64("secret")
        "SENDER",
        "TARGET",
        "A",
        1,
        "20260212-12:00:00.000",
        "pass",
    );

    // Wrong order: swap sender and target — should produce different signature
    let wrong_order = compute_logon_hmac(
        "c2VjcmV0", // base64("secret")
        "TARGET",   // Swapped
        "SENDER",   // Swapped
        "A",
        1,
        "20260212-12:00:00.000",
        "pass",
    );

    assert_ne!(
        correct_order, wrong_order,
        "HMAC signature order must match Coinbase spec"
    );
}

// ============================================================================
// CLIENT SESSION COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_client_session_starts_sequence_at_one() {
    let config = SessionConfig {
        sender_comp_id: "TEST".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 30,
    };

    let client = ClientSession::new(config);

    assert_eq!(
        client.current_seq_num_out(),
        1,
        "Client session must start with MsgSeqNum=1"
    );
}

#[test]
fn test_client_session_increments_sequence() {
    let config = SessionConfig {
        sender_comp_id: "TEST".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 30,
    };

    let mut client = ClientSession::new(config);

    // Create Logon (seq 1)
    let _logon = client.create_logon_message();
    assert_eq!(client.current_seq_num_out(), 2);

    // Create Heartbeat (seq 2)
    let _heartbeat = client.create_heartbeat_message(None);
    assert_eq!(client.current_seq_num_out(), 3);

    // Create TestRequest (seq 3)
    let _test_req = client.create_test_request_message("TEST");
    assert_eq!(client.current_seq_num_out(), 4);
}

#[test]
fn test_heartbeat_interval_maximum_300() {
    let config = SessionConfig {
        sender_comp_id: "TEST".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 300, // Maximum allowed
    };

    let client = ClientSession::new(config);
    let logon_msg = client.create_logon_message();
    let msg_str = String::from_utf8_lossy(&logon_msg);

    // Extract HeartBtInt
    let hb_start = msg_str.find("108=").expect("Missing HeartBtInt");
    let hb_end = msg_str[hb_start..].find('\x01').unwrap();
    let hb_value: u32 = msg_str[hb_start + 4..hb_start + hb_end]
        .parse()
        .expect("Invalid HeartBtInt");

    assert!(
        hb_value <= 300,
        "HeartBtInt must be <= 300 seconds, got {}",
        hb_value
    );
}

#[test]
fn test_test_request_includes_test_req_id() {
    let config = SessionConfig {
        sender_comp_id: "TEST".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 30,
    };

    let client = ClientSession::new(config);
    let test_req = client.create_test_request_message("TEST_ID_12345");
    let msg_str = String::from_utf8_lossy(&test_req);

    assert!(
        msg_str.contains("112=TEST_ID_12345\x01"),
        "TestRequest must include TestReqID(112)"
    );
}

#[test]
fn test_client_detects_sequence_gaps() {
    let config = SessionConfig {
        sender_comp_id: "TEST".to_string(),
        target_comp_id: "Coinbase".to_string(),
        api_secret: "c2VjcmV0".to_string(),
        api_passphrase: "pass".to_string(),
        heartbeat_interval: 30,
    };

    let mut client = ClientSession::new(config);

    // Receive message with seq 1 (expected)
    let msg1 = FixMessage::new(
        MsgType::Logon,
        1,
        "Coinbase".to_string(),
        "TEST".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    let serialized1 = build_fix_message(&msg1);
    client.feed_data(&serialized1);
    client.parse_next_message().unwrap().unwrap();
    assert!(client.check_sequence_number(1), "Seq 1 must be accepted");

    // Receive message with seq 3 (gap! expected 2)
    let msg3 = FixMessage::new(
        MsgType::Heartbeat,
        3, // Gap
        "Coinbase".to_string(),
        "TEST".to_string(),
        "20260212-12:00:01.000".to_string(),
    );
    let serialized3 = build_fix_message(&msg3);
    client.feed_data(&serialized3);
    client.parse_next_message().unwrap().unwrap();

    assert!(
        !client.check_sequence_number(3),
        "Client must detect sequence gap (expected 2, got 3)"
    );
}

// ============================================================================
// ROUND-TRIP COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_serialize_parse_roundtrip() {
    let original = FixMessage::new(
        MsgType::NewOrderSingle,
        42,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-14:30:45.123".to_string(),
    );

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, original.msg_type);
    assert_eq!(parsed.msg_seq_num, original.msg_seq_num);
    assert_eq!(parsed.sender_comp_id, original.sender_comp_id);
    assert_eq!(parsed.target_comp_id, original.target_comp_id);
    assert_eq!(parsed.sending_time, original.sending_time);
}

#[test]
fn test_roundtrip_with_all_field_types() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    // Add various field types
    msg.add_field(Tag::ClOrdID, "uuid-1234".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrderQty, "0.5".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string());

    let serialized = build_fix_message(&msg);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.get_field(Tag::ClOrdID), Some("uuid-1234"));
    assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC-USD"));
    assert_eq!(parsed.get_field(Tag::Side), Some("1"));
    assert_eq!(parsed.get_field(Tag::OrderQty), Some("0.5"));
    assert_eq!(parsed.get_field(Tag::Price), Some("50000.00"));
    assert_eq!(parsed.get_field(Tag::TimeInForce), Some("1"));
}

// ============================================================================
// PROTOCOL EDGE CASES
// ============================================================================

#[test]
fn test_parser_handles_malformed_without_panic() {
    let malformed_inputs = vec![
        b"GARBAGE".to_vec(),
        b"8=FIX.4.2\x01".to_vec(),
        b"8=FIXT.1.1\x019=NOTANUM\x01".to_vec(),
        vec![0xFF; 100],                           // Non-UTF8
        b"8=FIXT.1.1\x019=5\x0135=0\x01".to_vec(), // Too short
    ];

    for input in malformed_inputs {
        let mut parser = Parser::new();
        parser.feed(&input);
        let _result = parser.parse_message(); // Should not panic
    }
}

#[test]
fn test_message_with_special_characters() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    // Add field with special characters (but not SOH)
    msg.add_field(Tag::Symbol, "BTC/USD@EXCH".to_string());

    let serialized = build_fix_message(&msg);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC/USD@EXCH"));
}

#[test]
fn test_empty_field_values_allowed() {
    let mut msg = FixMessage::new(
        MsgType::ExecutionReport,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    // Empty value (valid in FIX)
    msg.add_field(Tag::Symbol, "".to_string());

    let serialized = build_fix_message(&msg);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.get_field(Tag::Symbol), Some(""));
}
