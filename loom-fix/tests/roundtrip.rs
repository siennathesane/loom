// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Round-trip property-based tests for FIX message serialization/parsing

use loom_fix::message::Tag;
use loom_fix::serialization::build_fix_message;
use loom_fix::{FixMessage, MsgType, Parser};
use proptest::prelude::*;

// Generate arbitrary MsgType
fn arb_msg_type() -> impl Strategy<Value = MsgType> {
    prop_oneof![
        Just(MsgType::Heartbeat),
        Just(MsgType::TestRequest),
        Just(MsgType::Logon),
        Just(MsgType::Logout),
        Just(MsgType::NewOrderSingle),
        Just(MsgType::ExecutionReport),
    ]
}

// Generate arbitrary printable ASCII string (FIX requires printable chars)
fn arb_fix_string() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_.-]{1,20}".prop_map(|s| s)
}

// Generate arbitrary timestamp string
fn arb_timestamp() -> impl Strategy<Value = String> {
    Just("20260212-12:00:00.000".to_string())
}

proptest! {
    #[test]
    fn test_roundtrip_serialization(
        msg_type in arb_msg_type(),
        seq_num in 1u64..1000,
        sender in arb_fix_string(),
        target in arb_fix_string(),
        timestamp in arb_timestamp(),
    ) {
        // Create a message
        let msg = FixMessage::new(
            msg_type,
            seq_num,
            sender.clone(),
            target.clone(),
            timestamp.clone(),
        );

        // Serialize it
        let serialized = build_fix_message(&msg);

        // Parse it back
        let mut parser = Parser::new();
        parser.feed(&serialized);

        let parsed = parser.parse_message()
            .expect("Should parse successfully")
            .expect("Should have a complete message");

        // Verify all fields match
        assert_eq!(parsed.msg_type, msg_type);
        assert_eq!(parsed.msg_seq_num, seq_num);
        assert_eq!(parsed.sender_comp_id, sender);
        assert_eq!(parsed.target_comp_id, target);
        assert_eq!(parsed.sending_time, timestamp);
    }

    #[test]
    fn test_roundtrip_with_fields(
        msg_type in arb_msg_type(),
        seq_num in 1u64..1000,
        sender in arb_fix_string(),
        target in arb_fix_string(),
        timestamp in arb_timestamp(),
        heartbeat_int in 10u32..120,
    ) {
        // Create a message with additional fields
        let mut msg = FixMessage::new(
            msg_type,
            seq_num,
            sender.clone(),
            target.clone(),
            timestamp.clone(),
        );

        msg.add_field(Tag::HeartBtInt, heartbeat_int.to_string());

        // Serialize it
        let serialized = build_fix_message(&msg);

        // Parse it back
        let mut parser = Parser::new();
        parser.feed(&serialized);

        let parsed = parser.parse_message()
            .expect("Should parse successfully")
            .expect("Should have a complete message");

        // Verify all fields match
        assert_eq!(parsed.msg_type, msg_type);
        assert_eq!(parsed.msg_seq_num, seq_num);
        assert_eq!(parsed.sender_comp_id, sender);
        assert_eq!(parsed.target_comp_id, target);
        assert_eq!(parsed.sending_time, timestamp);
        assert_eq!(parsed.get_field(Tag::HeartBtInt), Some(heartbeat_int.to_string().as_str()));
    }
}

#[test]
fn test_multiple_messages_in_stream() {
    // Create several messages
    let messages = vec![
        FixMessage::new(
            MsgType::Logon,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        ),
        FixMessage::new(
            MsgType::Heartbeat,
            2,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:01.000".to_string(),
        ),
        FixMessage::new(
            MsgType::Logout,
            3,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:02.000".to_string(),
        ),
    ];

    // Serialize all messages into one stream
    let mut stream = Vec::new();
    for msg in &messages {
        let serialized = build_fix_message(msg);
        stream.extend_from_slice(&serialized);
    }

    // Parse all messages from the stream
    let mut parser = Parser::new();
    parser.feed(&stream);

    for expected in &messages {
        let parsed = parser
            .parse_message()
            .expect("Should parse successfully")
            .expect("Should have a complete message");

        assert_eq!(parsed.msg_type, expected.msg_type);
        assert_eq!(parsed.msg_seq_num, expected.msg_seq_num);
    }

    // Should be no more messages
    assert!(parser.parse_message().unwrap().is_none());
}
