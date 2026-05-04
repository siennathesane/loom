// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use loom_fix::market_data::*;
use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use rust_decimal::Decimal;
use std::str::FromStr;

#[test]
fn test_market_data_request_roundtrip() {
    // Build a MarketDataRequest
    let request_bytes = MarketDataRequestBuilder::new(
        1,
        "CLIENT".to_string(),
        "COINBASE".to_string(),
        "20260212-12:00:00.000".to_string(),
        "REQ123".to_string(),
    )
    .subscription_request_type(SubscriptionRequestType::Subscribe)
    .market_depth(0)
    .md_update_type(MDUpdateType::IncrementalRefresh)
    .add_symbol("BTC-USD".to_string())
    .build();

    // Parse it back
    let mut parser = Parser::new();
    parser.feed(&request_bytes);
    let parsed = parser.parse_message().unwrap().unwrap();

    // Verify fields
    assert_eq!(parsed.msg_type, MsgType::MarketDataRequest);
    assert_eq!(parsed.sender_comp_id, "CLIENT");
    assert_eq!(parsed.target_comp_id, "COINBASE");
    assert_eq!(parsed.get_field(Tag::MDReqID), Some("REQ123"));
    assert_eq!(parsed.get_field(Tag::SubscriptionRequestType), Some("1")); // Subscribe
    assert_eq!(parsed.get_field(Tag::MarketDepth), Some("0"));
    assert_eq!(parsed.get_field(Tag::MDUpdateType), Some("1")); // Incremental
    assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC-USD"));
}

#[test]
fn test_security_list_request_roundtrip() {
    // Build a SecurityListRequest
    let request_bytes = SecurityListRequestBuilder::new(
        1,
        "CLIENT".to_string(),
        "COINBASE".to_string(),
        "20260212-12:00:00.000".to_string(),
        "SECREQ123".to_string(),
    )
    .build();

    // Parse it back
    let mut parser = Parser::new();
    parser.feed(&request_bytes);
    let parsed = parser.parse_message().unwrap().unwrap();

    // Verify fields
    assert_eq!(parsed.msg_type, MsgType::SecurityListRequest);
    assert_eq!(parsed.sender_comp_id, "CLIENT");
    assert_eq!(parsed.target_comp_id, "COINBASE");
    assert_eq!(parsed.get_field(Tag::SecurityReqID), Some("SECREQ123"));
}

#[test]
fn test_incremental_refresh_roundtrip() {
    // Create a MarketDataIncrementalRefresh message manually
    let mut msg = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        1,
        "COINBASE".to_string(),
        "CLIENT".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "REQ123".to_string());
    msg.add_field(Tag::NoMDEntries, "2".to_string());

    // First entry
    msg.add_field(Tag::MDUpdateAction, "0".to_string());
    msg.add_field(Tag::MDEntryType, "0".to_string());
    msg.add_field(Tag::MDEntryPx, "50000.00".to_string());
    msg.add_field(Tag::MDEntrySize, "1.5".to_string());
    msg.add_field(Tag::MDEntryID, "ORDER1".to_string());

    // Second entry
    msg.add_field(Tag::MDUpdateAction, "1".to_string());
    msg.add_field(Tag::MDEntryType, "1".to_string());
    msg.add_field(Tag::MDEntryPx, "50001.00".to_string());
    msg.add_field(Tag::MDEntrySize, "2.0".to_string());
    msg.add_field(Tag::MDEntryID, "ORDER2".to_string());

    // Parse it into structured form
    let refresh = MarketDataIncrementalRefresh::from_fix_message(&msg).unwrap();

    // Verify parsed data
    assert_eq!(refresh.md_req_id, Some("REQ123".to_string()));
    assert_eq!(refresh.entries.len(), 2);

    assert_eq!(refresh.entries[0].update_action, MDUpdateAction::New);
    assert_eq!(refresh.entries[0].entry_type, MDEntryType::Bid);
    assert_eq!(
        refresh.entries[0].price,
        Decimal::from_str("50000.00").unwrap()
    );
    assert_eq!(refresh.entries[0].size, Decimal::from_str("1.5").unwrap());
    assert_eq!(refresh.entries[0].entry_id, Some("ORDER1".to_string()));

    assert_eq!(refresh.entries[1].update_action, MDUpdateAction::Change);
    assert_eq!(refresh.entries[1].entry_type, MDEntryType::Offer);
    assert_eq!(
        refresh.entries[1].price,
        Decimal::from_str("50001.00").unwrap()
    );
    assert_eq!(refresh.entries[1].size, Decimal::from_str("2.0").unwrap());
}

#[test]
fn test_security_list_roundtrip() {
    // Create a SecurityList message manually
    let mut msg = FixMessage::new(
        MsgType::SecurityList,
        1,
        "COINBASE".to_string(),
        "CLIENT".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::SecurityReqID, "SECREQ1".to_string());
    msg.add_field(Tag::SecurityResponseID, "SECRESP1".to_string());
    msg.add_field(Tag::NoRelatedSym, "3".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Symbol, "ETH-USD".to_string());
    msg.add_field(Tag::Symbol, "SOL-USD".to_string());

    // Parse it into structured form
    let list = SecurityList::from_fix_message(&msg).unwrap();

    // Verify parsed data
    assert_eq!(list.security_req_id, "SECREQ1");
    assert_eq!(list.security_response_id, "SECRESP1");
    assert_eq!(list.symbols.len(), 3);
    assert_eq!(list.symbols[0], "BTC-USD");
    assert_eq!(list.symbols[1], "ETH-USD");
    assert_eq!(list.symbols[2], "SOL-USD");
}

#[test]
fn test_security_status_roundtrip() {
    // Create a SecurityStatus message manually
    let mut msg = FixMessage::new(
        MsgType::SecurityStatus,
        1,
        "COINBASE".to_string(),
        "CLIENT".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::SecurityTradingStatus, "2".to_string()); // Trading halt

    // Parse it into structured form
    let status = SecurityStatus::from_fix_message(&msg).unwrap();

    // Verify parsed data
    assert_eq!(status.symbol, "BTC-USD");
    assert_eq!(status.trading_status, TradingStatus::TradingHalt);
}
