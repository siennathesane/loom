// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Market Data Message Compliance Tests
//!
//! Validates that our implementation correctly handles Coinbase-specific
//! market data message requirements for:
//! - MarketDataRequest (subscription/unsubscription)
//! - MarketDataIncrementalRefresh (L3 order book updates)
//! - MarketDataSnapshotFullRefresh (full book snapshots)
//! - SecurityList (instrument definitions)
//! - SecurityStatus (trading status updates)
//!
//! Reference:
//! - https://docs.cdp.coinbase.com/exchange/fix-api/market-data

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use loom_fix::serialization::build_fix_message;

// ============================================================================
// MARKET DATA REQUEST COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_market_data_request_subscription() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataRequest,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "MD_REQ_001".to_string());
    msg.add_field(Tag::SubscriptionRequestType, "1".to_string()); // Subscribe
    msg.add_field(Tag::MarketDepth, "0".to_string()); // Full book
    msg.add_field(Tag::NoMDEntryTypes, "2".to_string());
    msg.add_field(Tag::NoRelatedSym, "1".to_string());

    assert_eq!(
        msg.get_field(Tag::MDReqID),
        Some("MD_REQ_001"),
        "MarketDataRequest must include MDReqID"
    );
    assert_eq!(
        msg.get_field(Tag::SubscriptionRequestType),
        Some("1"),
        "SubscriptionRequestType=1 for subscribe"
    );
}

#[test]
fn test_market_data_request_unsubscription() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataRequest,
        2,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "MD_REQ_002".to_string());
    msg.add_field(Tag::SubscriptionRequestType, "2".to_string()); // Unsubscribe

    assert_eq!(
        msg.get_field(Tag::SubscriptionRequestType),
        Some("2"),
        "SubscriptionRequestType=2 for unsubscribe"
    );
}

#[test]
fn test_market_data_request_with_symbol() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataRequest,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "MD_REQ_003".to_string());
    msg.add_field(Tag::SubscriptionRequestType, "1".to_string());
    msg.add_field(Tag::MarketDepth, "0".to_string());
    msg.add_field(Tag::NoMDEntryTypes, "2".to_string());
    msg.add_field(Tag::NoRelatedSym, "1".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());

    assert_eq!(
        msg.get_field(Tag::Symbol),
        Some("BTC-USD"),
        "MarketDataRequest should include Symbol"
    );
}

#[test]
fn test_market_data_request_serialization() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataRequest,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "MD_001".to_string());
    msg.add_field(Tag::SubscriptionRequestType, "1".to_string());
    msg.add_field(Tag::MarketDepth, "0".to_string());
    msg.add_field(Tag::NoMDEntryTypes, "2".to_string());
    msg.add_field(Tag::NoRelatedSym, "1".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    assert!(
        msg_str.contains("35=V"),
        "MsgType must be V (MarketDataRequest)"
    );
    assert!(msg_str.contains("262=MD_001"), "Must include MDReqID");
    assert!(
        msg_str.contains("263=1"),
        "Must include SubscriptionRequestType"
    );
    assert!(msg_str.contains("264=0"), "Must include MarketDepth");
}

// ============================================================================
// MARKET DATA INCREMENTAL REFRESH COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_market_data_incremental_refresh_structure() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        5,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::NoMDEntries, "1".to_string());
    msg.add_field(Tag::MDUpdateAction, "0".to_string()); // New
    msg.add_field(Tag::MDEntryType, "0".to_string()); // Bid
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::MDEntryPx, "50000.00".to_string());
    msg.add_field(Tag::MDEntrySize, "0.5".to_string());

    assert_eq!(msg.msg_type, MsgType::MarketDataIncrementalRefresh);
    assert_eq!(msg.get_field(Tag::NoMDEntries), Some("1"));
}

#[test]
fn test_market_data_update_action_types() {
    // MDUpdateAction: 0=New, 1=Change, 2=Delete
    let update_actions = vec![("0", "New"), ("1", "Change"), ("2", "Delete")];

    for (action_code, action_name) in update_actions {
        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "Coinbase".to_string(),
            "API_KEY".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::NoMDEntries, "1".to_string());
        msg.add_field(Tag::MDUpdateAction, action_code.to_string());
        msg.add_field(Tag::MDEntryType, "0".to_string());
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());

        assert_eq!(
            msg.get_field(Tag::MDUpdateAction),
            Some(action_code),
            "MDUpdateAction {} ({}) must be supported",
            action_code,
            action_name
        );
    }
}

#[test]
fn test_market_data_entry_types() {
    // MDEntryType: 0=Bid, 1=Offer, 2=Trade
    let entry_types = vec![("0", "Bid"), ("1", "Offer"), ("2", "Trade")];

    for (type_code, type_name) in entry_types {
        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "Coinbase".to_string(),
            "API_KEY".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::NoMDEntries, "1".to_string());
        msg.add_field(Tag::MDUpdateAction, "0".to_string());
        msg.add_field(Tag::MDEntryType, type_code.to_string());
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());

        assert_eq!(
            msg.get_field(Tag::MDEntryType),
            Some(type_code),
            "MDEntryType {} ({}) must be supported",
            type_code,
            type_name
        );
    }
}

#[test]
fn test_market_data_with_entry_id() {
    // Per spec: MDEntryID present = book-building update
    let mut msg = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::NoMDEntries, "1".to_string());
    msg.add_field(Tag::MDUpdateAction, "0".to_string());
    msg.add_field(Tag::MDEntryType, "0".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::MDEntryID, "order_12345".to_string()); // Order ID
    msg.add_field(Tag::MDEntryPx, "50000.00".to_string());
    msg.add_field(Tag::MDEntrySize, "0.5".to_string());

    assert!(
        msg.get_field(Tag::MDEntryID).is_some(),
        "Book-building updates include MDEntryID"
    );
}

#[test]
fn test_market_data_incremental_refresh_roundtrip() {
    let mut original = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        10,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    original.add_field(Tag::NoMDEntries, "1".to_string());
    original.add_field(Tag::MDUpdateAction, "0".to_string());
    original.add_field(Tag::MDEntryType, "0".to_string());
    original.add_field(Tag::Symbol, "BTC-USD".to_string());
    original.add_field(Tag::MDEntryPx, "50123.45".to_string());
    original.add_field(Tag::MDEntrySize, "1.23456789".to_string());
    original.add_field(Tag::MDEntryID, "order_abc123".to_string());

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::MarketDataIncrementalRefresh);
    assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC-USD"));
    assert_eq!(parsed.get_field(Tag::MDEntryPx), Some("50123.45"));
    assert_eq!(parsed.get_field(Tag::MDEntryID), Some("order_abc123"));
}

// ============================================================================
// MARKET DATA SNAPSHOT FULL REFRESH COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_market_data_snapshot_structure() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataSnapshotFullRefresh,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::NoMDEntries, "2".to_string());

    assert_eq!(msg.msg_type, MsgType::MarketDataSnapshotFullRefresh);
    assert_eq!(msg.get_field(Tag::Symbol), Some("BTC-USD"));
}

#[test]
fn test_market_data_snapshot_serialization() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataSnapshotFullRefresh,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::Symbol, "ETH-USD".to_string());
    msg.add_field(Tag::NoMDEntries, "10".to_string());

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    assert!(
        msg_str.contains("35=W"),
        "MsgType must be W (MarketDataSnapshotFullRefresh)"
    );
    assert!(msg_str.contains("55=ETH-USD"), "Must include Symbol");
    assert!(msg_str.contains("268=10"), "Must include NoMDEntries");
}

#[test]
fn test_market_data_snapshot_roundtrip() {
    let mut original = FixMessage::new(
        MsgType::MarketDataSnapshotFullRefresh,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    original.add_field(Tag::Symbol, "BTC-USD".to_string());
    original.add_field(Tag::NoMDEntries, "5".to_string());

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::MarketDataSnapshotFullRefresh);
    assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC-USD"));
    assert_eq!(parsed.get_field(Tag::NoMDEntries), Some("5"));
}

// ============================================================================
// SECURITY LIST COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_security_list_request_structure() {
    let mut msg = FixMessage::new(
        MsgType::SecurityListRequest,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::SecurityReqID, "SEC_REQ_001".to_string());
    msg.add_field(Tag::SecurityListRequestType, "0".to_string()); // Symbol

    assert_eq!(msg.msg_type, MsgType::SecurityListRequest);
    assert_eq!(
        msg.get_field(Tag::SecurityReqID),
        Some("SEC_REQ_001"),
        "SecurityListRequest must include SecurityReqID"
    );
}

#[test]
fn test_security_list_response_structure() {
    let mut msg = FixMessage::new(
        MsgType::SecurityList,
        5,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::SecurityReqID, "SEC_REQ_001".to_string());
    msg.add_field(Tag::SecurityResponseID, "SEC_RESP_001".to_string());
    msg.add_field(Tag::NoRelatedSym, "50".to_string());

    assert_eq!(msg.msg_type, MsgType::SecurityList);
    assert_eq!(
        msg.get_field(Tag::SecurityReqID),
        Some("SEC_REQ_001"),
        "SecurityList must echo SecurityReqID"
    );
}

#[test]
fn test_security_list_roundtrip() {
    let mut original = FixMessage::new(
        MsgType::SecurityList,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    original.add_field(Tag::SecurityReqID, "SEC_001".to_string());
    original.add_field(Tag::SecurityResponseID, "RESP_001".to_string());
    original.add_field(Tag::NoRelatedSym, "100".to_string());

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::SecurityList);
    assert_eq!(parsed.get_field(Tag::SecurityReqID), Some("SEC_001"));
    assert_eq!(parsed.get_field(Tag::NoRelatedSym), Some("100"));
}

// ============================================================================
// SECURITY STATUS COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_security_status_message_structure() {
    let mut msg = FixMessage::new(
        MsgType::SecurityStatus,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::SecurityTradingStatus, "17".to_string()); // Trading halt

    assert_eq!(msg.msg_type, MsgType::SecurityStatus);
    assert_eq!(msg.get_field(Tag::Symbol), Some("BTC-USD"));
}

#[test]
fn test_security_status_trading_states() {
    // Per spec: trading_disabled, cancel_only, post_only, limit_only, full_trading, auction_mode
    // These map to numeric values in SecurityTradingStatus
    let trading_states = vec![
        "2",  // Trading halt
        "17", // Ready to trade
        "18", // Not available for trading
        "20", // Unknown
        "21", // Pre-open
        "22", // Opening auction
    ];

    for state in trading_states {
        let mut msg = FixMessage::new(
            MsgType::SecurityStatus,
            1,
            "Coinbase".to_string(),
            "API_KEY".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::SecurityTradingStatus, state.to_string());

        assert_eq!(
            msg.get_field(Tag::SecurityTradingStatus),
            Some(state),
            "SecurityStatus must support trading state {}",
            state
        );
    }
}

#[test]
fn test_security_status_roundtrip() {
    let mut original = FixMessage::new(
        MsgType::SecurityStatus,
        1,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    original.add_field(Tag::Symbol, "ETH-USD".to_string());
    original.add_field(Tag::SecurityTradingStatus, "17".to_string());

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::SecurityStatus);
    assert_eq!(parsed.get_field(Tag::Symbol), Some("ETH-USD"));
    assert_eq!(parsed.get_field(Tag::SecurityTradingStatus), Some("17"));
}

// ============================================================================
// MARKET DATA REQUEST REJECT COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_market_data_request_reject_structure() {
    let mut msg = FixMessage::new(
        MsgType::MarketDataRequestReject,
        5,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "MD_REQ_001".to_string());
    msg.add_field(Tag::Symbol, "INVALID-SYMBOL".to_string());

    assert_eq!(msg.msg_type, MsgType::MarketDataRequestReject);
    assert_eq!(
        msg.get_field(Tag::MDReqID),
        Some("MD_REQ_001"),
        "Reject must echo MDReqID"
    );
}

// ============================================================================
// MARKET DATA WORKFLOW TESTS
// ============================================================================

#[test]
fn test_market_data_subscription_workflow() {
    // 1. Client sends MarketDataRequest
    let mut request = FixMessage::new(
        MsgType::MarketDataRequest,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    request.add_field(Tag::MDReqID, "MD_WORKFLOW_001".to_string());
    request.add_field(Tag::SubscriptionRequestType, "1".to_string());
    request.add_field(Tag::MarketDepth, "0".to_string());
    request.add_field(Tag::NoRelatedSym, "1".to_string());
    request.add_field(Tag::Symbol, "BTC-USD".to_string());

    let req_serialized = build_fix_message(&request);

    // Verify request serializes correctly
    let mut parser = Parser::new();
    parser.feed(&req_serialized);
    let parsed_req = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed_req.msg_type, MsgType::MarketDataRequest);

    // 2. Server sends MarketDataIncrementalRefresh
    let mut refresh = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        2,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:01.000".to_string(),
    );

    refresh.add_field(Tag::NoMDEntries, "1".to_string());
    refresh.add_field(Tag::MDUpdateAction, "0".to_string());
    refresh.add_field(Tag::MDEntryType, "0".to_string());
    refresh.add_field(Tag::Symbol, "BTC-USD".to_string());
    refresh.add_field(Tag::MDEntryPx, "50000.00".to_string());
    refresh.add_field(Tag::MDEntrySize, "1.0".to_string());

    let refresh_serialized = build_fix_message(&refresh);

    // Verify refresh serializes correctly
    parser.clear();
    parser.feed(&refresh_serialized);
    let parsed_refresh = parser.parse_message().unwrap().unwrap();
    assert_eq!(
        parsed_refresh.msg_type,
        MsgType::MarketDataIncrementalRefresh
    );
}

#[test]
fn test_security_list_workflow() {
    // 1. Client sends SecurityListRequest
    let mut request = FixMessage::new(
        MsgType::SecurityListRequest,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    request.add_field(Tag::SecurityReqID, "SEC_WORKFLOW_001".to_string());
    request.add_field(Tag::SecurityListRequestType, "0".to_string());

    let req_serialized = build_fix_message(&request);

    // 2. Server sends SecurityList
    let mut response = FixMessage::new(
        MsgType::SecurityList,
        2,
        "Coinbase".to_string(),
        "API_KEY".to_string(),
        "20260212-12:00:01.000".to_string(),
    );

    response.add_field(Tag::SecurityReqID, "SEC_WORKFLOW_001".to_string());
    response.add_field(Tag::SecurityResponseID, "RESP_001".to_string());
    response.add_field(Tag::NoRelatedSym, "50".to_string());

    let resp_serialized = build_fix_message(&response);

    // Verify workflow messages serialize/parse correctly
    let mut parser = Parser::new();
    parser.feed(&req_serialized);
    let parsed_req = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed_req.msg_type, MsgType::SecurityListRequest);

    parser.clear();
    parser.feed(&resp_serialized);
    let parsed_resp = parser.parse_message().unwrap().unwrap();
    assert_eq!(parsed_resp.msg_type, MsgType::SecurityList);
    assert_eq!(
        parsed_resp.get_field(Tag::SecurityReqID),
        Some("SEC_WORKFLOW_001"),
        "SecurityList must echo SecurityReqID from request"
    );
}
