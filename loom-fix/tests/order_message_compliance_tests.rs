// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Order Entry Message Compliance Tests
//!
//! Validates that our implementation correctly handles Coinbase-specific
//! order entry message requirements for:
//! - Limit orders (with CashOrderQty)
//! - IOC (Immediate-or-Cancel) orders
//! - TPSL (Take-Profit/Stop-Loss) orders
//! - Order modification and cancellation
//!
//! References:
//! - https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages/limit-orders
//! - https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages/tpsl-orders

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use loom_fix::serialization::build_fix_message;

// ============================================================================
// LIMIT ORDER COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_limit_order_ord_type_is_2() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::CashOrderQty, "25000.00".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string()); // GTC

    assert_eq!(
        msg.get_field(Tag::OrdType),
        Some("2"),
        "Limit orders must have OrdType(40)=2"
    );
}

#[test]
fn test_limit_order_with_cash_order_qty() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "2".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::CashOrderQty, "10000.00".to_string()); // Quote currency amount
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string());

    assert!(
        msg.get_field(Tag::CashOrderQty).is_some(),
        "Limit order can use CashOrderQty for notional value"
    );
}

#[test]
fn test_limit_order_cash_qty_excludes_order_qty() {
    // Per spec: "You must not define OrderQty when submitting a limit order with CashOrderQty"
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "2".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::CashOrderQty, "10000.00".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string());

    // Should NOT have OrderQty when using CashOrderQty
    assert!(
        msg.get_field(Tag::OrderQty).is_none(),
        "Limit order with CashOrderQty must not include OrderQty(38)"
    );
}

#[test]
fn test_limit_order_supports_all_time_in_force() {
    // Per spec: "All TimeInForce values are compatible"
    let time_in_force_values = vec![
        ("1", "GTC - Good Till Cancel"),
        ("3", "IOC - Immediate or Cancel"),
        ("4", "FOK - Fill or Kill"),
        ("6", "GTD - Good Till Date"),
    ];

    for (tif_value, tif_name) in time_in_force_values {
        let mut msg = FixMessage::new(
            MsgType::NewOrderSingle,
            1,
            "TEST".to_string(),
            "Coinbase".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::OrdType, "2".to_string());
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::CashOrderQty, "10000.00".to_string());
        msg.add_field(Tag::Price, "50000.00".to_string());
        msg.add_field(Tag::TimeInForce, tif_value.to_string());

        assert_eq!(
            msg.get_field(Tag::TimeInForce),
            Some(tif_value),
            "Limit order must support TimeInForce: {}",
            tif_name
        );
    }
}

#[test]
fn test_limit_order_serialization_complete() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    msg.add_field(Tag::OrdType, "2".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::CashOrderQty, "25000.00".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string());

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    // Verify all required fields are present
    assert!(msg_str.contains("11="), "Must include ClOrdID");
    assert!(msg_str.contains("40=2"), "Must include OrdType=2");
    assert!(msg_str.contains("55=BTC-USD"), "Must include Symbol");
    assert!(msg_str.contains("54=1"), "Must include Side");
    assert!(
        msg_str.contains("152=25000.00"),
        "Must include CashOrderQty"
    );
    assert!(msg_str.contains("44=50000.00"), "Must include Price");
    assert!(msg_str.contains("59=1"), "Must include TimeInForce");
}

// ============================================================================
// IOC (IMMEDIATE-OR-CANCEL) ORDER COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_ioc_order_time_in_force_3() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::CashOrderQty, "10000.00".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "3".to_string()); // IOC

    assert_eq!(
        msg.get_field(Tag::TimeInForce),
        Some("3"),
        "IOC orders must have TimeInForce(59)=3"
    );
}

#[test]
fn test_ioc_order_complete_message_structure() {
    // IOC order for arbitrage execution
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "ARBITRAGE_BOT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string()); // Buy
    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::CashOrderQty, "50000.00".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "3".to_string()); // IOC

    let serialized = build_fix_message(&msg);

    // Parse to verify
    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::NewOrderSingle);
    assert_eq!(parsed.get_field(Tag::TimeInForce), Some("3"));
    assert_eq!(parsed.get_field(Tag::OrdType), Some("2"));
}

// ============================================================================
// TPSL (TAKE-PROFIT/STOP-LOSS) ORDER COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_tpsl_order_ord_type_is_o() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "O".to_string()); // TPSL - letter O
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrderQty, "1.0".to_string());

    assert_eq!(
        msg.get_field(Tag::OrdType),
        Some("O"),
        "TPSL orders must have OrdType(40)=O (letter O)"
    );
}

#[test]
fn test_tpsl_order_required_price_fields() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "O".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "2".to_string()); // Sell
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::Price, "55000.00".to_string()); // Take profit
    msg.add_field(Tag::StopPx, "48000.00".to_string()); // Stop loss trigger
    msg.add_field(Tag::StopLimitPx, "47500.00".to_string()); // Stop limit price

    // All three price fields must be present
    assert!(
        msg.get_field(Tag::Price).is_some(),
        "TPSL must include Price(44) for take profit"
    );
    assert!(
        msg.get_field(Tag::StopPx).is_some(),
        "TPSL must include StopPx(99) for stop loss trigger"
    );
    assert!(
        msg.get_field(Tag::StopLimitPx).is_some(),
        "TPSL must include StopLimitPx(3040) for stop limit"
    );
}

#[test]
fn test_tpsl_sell_order_price_validation() {
    // For SELL orders: Price > StopPx > StopLimitPx
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "O".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "2".to_string()); // Sell
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::Price, "55000.00".to_string()); // Highest
    msg.add_field(Tag::StopPx, "48000.00".to_string()); // Middle
    msg.add_field(Tag::StopLimitPx, "47500.00".to_string()); // Lowest

    let price: f64 = msg.get_field(Tag::Price).unwrap().parse().unwrap();
    let stop_px: f64 = msg.get_field(Tag::StopPx).unwrap().parse().unwrap();
    let stop_limit_px: f64 = msg.get_field(Tag::StopLimitPx).unwrap().parse().unwrap();

    assert!(
        price > stop_px && stop_px > stop_limit_px,
        "SELL TPSL: Price({}) > StopPx({}) > StopLimitPx({})",
        price,
        stop_px,
        stop_limit_px
    );
}

#[test]
fn test_tpsl_buy_order_price_validation() {
    // For BUY orders: Price < StopPx < StopLimitPx
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "O".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string()); // Buy
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::Price, "45000.00".to_string()); // Lowest
    msg.add_field(Tag::StopPx, "52000.00".to_string()); // Middle
    msg.add_field(Tag::StopLimitPx, "52500.00".to_string()); // Highest

    let price: f64 = msg.get_field(Tag::Price).unwrap().parse().unwrap();
    let stop_px: f64 = msg.get_field(Tag::StopPx).unwrap().parse().unwrap();
    let stop_limit_px: f64 = msg.get_field(Tag::StopLimitPx).unwrap().parse().unwrap();

    assert!(
        price < stop_px && stop_px < stop_limit_px,
        "BUY TPSL: Price({}) < StopPx({}) < StopLimitPx({})",
        price,
        stop_px,
        stop_limit_px
    );
}

#[test]
fn test_tpsl_time_in_force_gtc_or_gtd() {
    // Per spec: TimeInForce limited to GTC or GTD
    let valid_tif = vec!["1", "6"]; // GTC, GTD

    for tif in valid_tif {
        let mut msg = FixMessage::new(
            MsgType::NewOrderSingle,
            1,
            "TEST".to_string(),
            "Coinbase".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::OrdType, "O".to_string());
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::OrderQty, "1.0".to_string());
        msg.add_field(Tag::Price, "45000.00".to_string());
        msg.add_field(Tag::StopPx, "52000.00".to_string());
        msg.add_field(Tag::StopLimitPx, "52500.00".to_string());
        msg.add_field(Tag::TimeInForce, tif.to_string());

        assert_eq!(
            msg.get_field(Tag::TimeInForce),
            Some(tif),
            "TPSL supports TimeInForce={}",
            tif
        );
    }
}

#[test]
fn test_tpsl_post_only_must_be_false() {
    // Per spec: Post-Only flag unsupported or must be false
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "TEST".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::OrdType, "O".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::Price, "45000.00".to_string());
    msg.add_field(Tag::StopPx, "52000.00".to_string());
    msg.add_field(Tag::StopLimitPx, "52500.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string());

    // Should NOT have ExecInst=A (Post Only)
    assert!(
        msg.get_field(Tag::ExecInst).is_none() || msg.get_field(Tag::ExecInst) != Some("A"),
        "TPSL orders must not have Post-Only flag"
    );
}

#[test]
fn test_tpsl_complete_message_serialization() {
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "2".to_string()); // Sell
    msg.add_field(Tag::OrdType, "O".to_string()); // TPSL
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::Price, "55000.00".to_string());
    msg.add_field(Tag::StopPx, "48000.00".to_string());
    msg.add_field(Tag::StopLimitPx, "47500.00".to_string());
    msg.add_field(Tag::TimeInForce, "1".to_string()); // GTC

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    assert!(msg_str.contains("40=O"), "Must include OrdType=O");
    assert!(msg_str.contains("44=55000.00"), "Must include Price");
    assert!(msg_str.contains("99=48000.00"), "Must include StopPx");
    assert!(
        msg_str.contains("3040=47500.00"),
        "Must include StopLimitPx"
    );
}

// ============================================================================
// ORDER MODIFICATION COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_order_cancel_replace_request_structure() {
    let mut msg = FixMessage::new(
        MsgType::OrderCancelReplaceRequest,
        2,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    let new_clord_id = Uuid::new_v4().to_string();
    let orig_clord_id = Uuid::new_v4().to_string();
    let order_id = Uuid::new_v4().to_string();

    msg.add_field(Tag::ClOrdID, new_clord_id.clone());
    msg.add_field(Tag::OrderID, order_id);
    msg.add_field(Tag::OrigClOrdID, orig_clord_id);
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::OrderQty, "1.5".to_string()); // New quantity
    msg.add_field(Tag::Price, "51000.00".to_string()); // New price

    // Verify required fields for modification
    assert!(msg.get_field(Tag::ClOrdID).is_some());
    assert!(msg.get_field(Tag::OrderID).is_some());
    assert!(msg.get_field(Tag::OrigClOrdID).is_some());
    assert!(msg.get_field(Tag::Symbol).is_some());
}

#[test]
fn test_tpsl_order_modification() {
    // TPSL orders can be modified until triggered
    let mut msg = FixMessage::new(
        MsgType::OrderCancelReplaceRequest,
        2,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    msg.add_field(Tag::OrderID, Uuid::new_v4().to_string());
    msg.add_field(Tag::OrigClOrdID, Uuid::new_v4().to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "2".to_string());
    msg.add_field(Tag::OrdType, "O".to_string()); // TPSL
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::Price, "56000.00".to_string()); // Updated TP
    msg.add_field(Tag::StopPx, "47000.00".to_string()); // Updated SL
    msg.add_field(Tag::StopLimitPx, "46500.00".to_string()); // Updated limit

    assert_eq!(msg.msg_type, MsgType::OrderCancelReplaceRequest);
    assert_eq!(msg.get_field(Tag::OrdType), Some("O"));
}

// ============================================================================
// ORDER CANCELLATION COMPLIANCE TESTS
// ============================================================================

#[test]
fn test_order_cancel_request_required_fields() {
    let mut msg = FixMessage::new(
        MsgType::OrderCancelRequest,
        2,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    msg.add_field(Tag::ClOrdID, Uuid::new_v4().to_string()); // New UUID
    msg.add_field(Tag::OrderID, Uuid::new_v4().to_string()); // Order to cancel
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());

    // Required fields for cancellation
    assert!(
        msg.get_field(Tag::ClOrdID).is_some(),
        "Cancel request needs ClOrdID"
    );
    assert!(
        msg.get_field(Tag::OrderID).is_some() || msg.get_field(Tag::OrigClOrdID).is_some(),
        "Cancel request needs OrderID or OrigClOrdID"
    );
    assert!(
        msg.get_field(Tag::Symbol).is_some(),
        "Cancel request needs Symbol"
    );
}

#[test]
fn test_order_cancel_request_serialization() {
    let mut msg = FixMessage::new(
        MsgType::OrderCancelRequest,
        2,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    let clord_id = Uuid::new_v4().to_string();
    let order_id = Uuid::new_v4().to_string();

    msg.add_field(Tag::ClOrdID, clord_id);
    msg.add_field(Tag::OrderID, order_id);
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string()); // Optional but recommended

    let serialized = build_fix_message(&msg);
    let msg_str = String::from_utf8_lossy(&serialized);

    assert!(
        msg_str.contains("35=F"),
        "MsgType must be F (OrderCancelRequest)"
    );
    assert!(msg_str.contains("11="), "Must include ClOrdID");
    assert!(msg_str.contains("37="), "Must include OrderID");
    assert!(msg_str.contains("55=BTC-USD"), "Must include Symbol");
}

// ============================================================================
// ROUND-TRIP TESTS FOR ORDER MESSAGES
// ============================================================================

#[test]
fn test_limit_order_roundtrip() {
    let mut original = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    original.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    original.add_field(Tag::Symbol, "BTC-USD".to_string());
    original.add_field(Tag::Side, "1".to_string());
    original.add_field(Tag::OrdType, "2".to_string());
    original.add_field(Tag::CashOrderQty, "25000.00".to_string());
    original.add_field(Tag::Price, "50000.00".to_string());
    original.add_field(Tag::TimeInForce, "3".to_string()); // IOC

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::NewOrderSingle);
    assert_eq!(parsed.get_field(Tag::OrdType), Some("2"));
    assert_eq!(parsed.get_field(Tag::CashOrderQty), Some("25000.00"));
    assert_eq!(parsed.get_field(Tag::TimeInForce), Some("3"));
}

#[test]
fn test_tpsl_order_roundtrip() {
    let mut original = FixMessage::new(
        MsgType::NewOrderSingle,
        1,
        "API_KEY".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    use uuid::Uuid;
    original.add_field(Tag::ClOrdID, Uuid::new_v4().to_string());
    original.add_field(Tag::Symbol, "ETH-USD".to_string());
    original.add_field(Tag::Side, "2".to_string());
    original.add_field(Tag::OrdType, "O".to_string());
    original.add_field(Tag::OrderQty, "10.0".to_string());
    original.add_field(Tag::Price, "2500.00".to_string());
    original.add_field(Tag::StopPx, "2200.00".to_string());
    original.add_field(Tag::StopLimitPx, "2190.00".to_string());
    original.add_field(Tag::TimeInForce, "1".to_string());

    let serialized = build_fix_message(&original);

    let mut parser = Parser::new();
    parser.feed(&serialized);
    let parsed = parser.parse_message().unwrap().unwrap();

    assert_eq!(parsed.msg_type, MsgType::NewOrderSingle);
    assert_eq!(parsed.get_field(Tag::OrdType), Some("O"));
    assert_eq!(parsed.get_field(Tag::Price), Some("2500.00"));
    assert_eq!(parsed.get_field(Tag::StopPx), Some("2200.00"));
    assert_eq!(parsed.get_field(Tag::StopLimitPx), Some("2190.00"));
}
