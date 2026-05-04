// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Order entry FIX protocol tests.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::parser::Parser;
use loom_fix::serialization::build_fix_message;
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
async fn test_new_order_single_filled() {
    let (mut stream, client, _addr) = setup_authenticated_connection().await;

    // Create NewOrderSingle
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    msg.add_field(Tag::ClOrdID, "ORDER_1".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string()); // Buy
    msg.add_field(Tag::OrderQty, "1.5".to_string());
    msg.add_field(Tag::OrdType, "2".to_string()); // Limit
    msg.add_field(Tag::Price, "50000.00".to_string());
    msg.add_field(Tag::TimeInForce, "3".to_string()); // IOC

    let order = build_fix_message(&msg);
    stream.write_all(&order).await.unwrap();

    // Should receive ExecutionReport (filled)
    let mut buffer = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::ExecutionReport);
    assert_eq!(response.get_field(Tag::ClOrdID), Some("ORDER_1"));
    assert_eq!(response.get_field(Tag::ExecType), Some("F")); // Fill
    assert_eq!(response.get_field(Tag::OrdStatus), Some("2")); // Filled
    assert_eq!(response.get_field(Tag::Symbol), Some("BTC-USD"));
}

#[tokio::test]
async fn test_new_order_single_rejected() {
    // Start server configured to reject orders
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    // Configure order book to reject
    {
        let ob = server.order_book.clone();
        let mut ob_guard = ob.write().await;
        ob_guard.set_reject_orders(true);
    }

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

    // Create NewOrderSingle
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    msg.add_field(Tag::ClOrdID, "ORDER_REJECT".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrderQty, "100.0".to_string());
    msg.add_field(Tag::OrdType, "2".to_string());
    msg.add_field(Tag::Price, "50000.00".to_string());

    let order = build_fix_message(&msg);
    stream.write_all(&order).await.unwrap();

    // Should receive ExecutionReport (rejected)
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::ExecutionReport);
    assert_eq!(response.get_field(Tag::ExecType), Some("8")); // Rejected
    assert_eq!(response.get_field(Tag::OrdStatus), Some("8")); // Rejected
}

#[tokio::test]
async fn test_order_cancel_request() {
    let (mut stream, client, _addr) = setup_authenticated_connection().await;

    // First, submit an order
    let mut new_order = FixMessage::new(
        MsgType::NewOrderSingle,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    new_order.add_field(Tag::ClOrdID, "ORDER_TO_CANCEL".to_string());
    new_order.add_field(Tag::Symbol, "ETH-USD".to_string());
    new_order.add_field(Tag::Side, "2".to_string()); // Sell
    new_order.add_field(Tag::OrderQty, "5.0".to_string());
    new_order.add_field(Tag::OrdType, "2".to_string());
    new_order.add_field(Tag::Price, "3000.00".to_string());

    let order = build_fix_message(&new_order);
    stream.write_all(&order).await.unwrap();

    let mut buffer = vec![0u8; 4096];
    let _ = stream.read(&mut buffer).await.unwrap();

    // Now send cancel request
    let mut cancel = FixMessage::new(
        MsgType::OrderCancelRequest,
        client.current_seq_num_out() + 1,
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:01.000".to_string(),
    );
    cancel.add_field(Tag::ClOrdID, "CANCEL_1".to_string());
    cancel.add_field(Tag::Symbol, "ETH-USD".to_string());

    let cancel_msg = build_fix_message(&cancel);
    stream.write_all(&cancel_msg).await.unwrap();

    // Should receive ExecutionReport (cancelled)
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::ExecutionReport);
    assert_eq!(response.get_field(Tag::ExecType), Some("4")); // Cancelled
}

#[tokio::test]
async fn test_order_lifecycle() {
    let (mut stream, client, _addr) = setup_authenticated_connection().await;

    // Submit order
    let mut msg = FixMessage::new(
        MsgType::NewOrderSingle,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    msg.add_field(Tag::ClOrdID, "LIFECYCLE_ORDER".to_string());
    msg.add_field(Tag::Symbol, "SOL-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrderQty, "10.0".to_string());
    msg.add_field(Tag::OrdType, "2".to_string());
    msg.add_field(Tag::Price, "100.00".to_string());

    let order = build_fix_message(&msg);
    stream.write_all(&order).await.unwrap();

    // Receive fill
    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let fill = parser.parse_message().unwrap().unwrap();

    assert_eq!(fill.msg_type, MsgType::ExecutionReport);
    assert_eq!(fill.get_field(Tag::ClOrdID), Some("LIFECYCLE_ORDER"));

    // Verify filled quantity matches requested (may be "10" or "10.0")
    let last_qty = fill.get_field(Tag::LastQty).unwrap();
    let qty_value: f64 = last_qty.parse().unwrap();
    assert_eq!(qty_value, 10.0);
}

#[tokio::test]
async fn test_multiple_orders_in_sequence() {
    let (mut stream, client, _addr) = setup_authenticated_connection().await;

    for i in 1..=5 {
        let mut msg = FixMessage::new(
            MsgType::NewOrderSingle,
            client.current_seq_num_out() + i - 1,
            "TEST_CLIENT".to_string(),
            "Coinbase".to_string(),
            format!("20260212-12:00:{:02}.000", i),
        );
        msg.add_field(Tag::ClOrdID, format!("ORDER_{}", i));
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::OrderQty, "0.1".to_string());
        msg.add_field(Tag::OrdType, "2".to_string());
        msg.add_field(Tag::Price, "50000.00".to_string());

        let order = build_fix_message(&msg);
        stream.write_all(&order).await.unwrap();

        // Read response
        let mut buffer = vec![0u8; 4096];
        let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
            .await
            .unwrap()
            .unwrap();

        let mut parser = Parser::new();
        parser.feed(&buffer[..n]);
        let response = parser.parse_message().unwrap().unwrap();

        assert_eq!(response.msg_type, MsgType::ExecutionReport);
        let expected_cl_ord_id = format!("ORDER_{}", i);
        assert_eq!(
            response.get_field(Tag::ClOrdID),
            Some(&expected_cl_ord_id as &str)
        );
    }
}
