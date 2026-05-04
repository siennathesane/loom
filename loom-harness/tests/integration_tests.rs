// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Integration tests for full FIX session lifecycles.

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

#[tokio::test]
async fn test_full_session_lifecycle() {
    // Complete lifecycle: connect → logon → subscribe → trade → logout

    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    // Configure instruments
    server
        .configure_market_data(vec!["BTC-USD".to_string(), "ETH-USD".to_string()])
        .await;

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    // 1. Connect and Logon
    stream
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let n = stream.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let logon_resp = parser.parse_message().unwrap().unwrap();
    assert_eq!(logon_resp.msg_type, MsgType::Logon);

    // 2. Subscribe to market data
    let mut md_req = FixMessage::new(
        MsgType::MarketDataRequest,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    md_req.add_field(Tag::MDReqID, "MD_1".to_string());
    md_req.add_field(Tag::SubscriptionRequestType, "1".to_string());

    stream.write_all(&build_fix_message(&md_req)).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    // 3. Place an order
    let mut order = FixMessage::new(
        MsgType::NewOrderSingle,
        client.current_seq_num_out() + 1,
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:01.000".to_string(),
    );
    order.add_field(Tag::ClOrdID, "FULL_CYCLE_ORDER".to_string());
    order.add_field(Tag::Symbol, "BTC-USD".to_string());
    order.add_field(Tag::Side, "1".to_string());
    order.add_field(Tag::OrderQty, "0.5".to_string());
    order.add_field(Tag::OrdType, "2".to_string());
    order.add_field(Tag::Price, "50000.00".to_string());

    stream.write_all(&build_fix_message(&order)).await.unwrap();

    // Receive fill
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    parser.feed(&buffer[..n]);
    let exec_report = parser.parse_message().unwrap().unwrap();
    assert_eq!(exec_report.msg_type, MsgType::ExecutionReport);
    assert_eq!(exec_report.get_field(Tag::ExecType), Some("F"));

    // 4. Logout
    stream
        .write_all(&client.create_logout_message())
        .await
        .unwrap();
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    parser.feed(&buffer[..n]);
    let logout_resp = parser.parse_message().unwrap().unwrap();
    assert_eq!(logout_resp.msg_type, MsgType::Logout);
}

#[tokio::test]
async fn test_reconnection_after_disconnect() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    // First connection
    let mut stream1 = TcpStream::connect(addr).await.unwrap();
    let client = ClientSession::new(test_client_config());

    stream1
        .write_all(&client.create_logon_message())
        .await
        .unwrap();
    let mut buffer = vec![0u8; 4096];
    let _ = stream1.read(&mut buffer).await.unwrap();

    // Close connection
    drop(stream1);
    sleep(Duration::from_millis(100)).await;

    // Reconnect
    let mut stream2 = TcpStream::connect(addr).await.unwrap();
    let client2 = ClientSession::new(test_client_config());

    stream2
        .write_all(&client2.create_logon_message())
        .await
        .unwrap();
    let n = stream2.read(&mut buffer).await.unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let logon_resp = parser.parse_message().unwrap().unwrap();

    assert_eq!(logon_resp.msg_type, MsgType::Logon);
    // Sequence should reset to 1
    assert_eq!(logon_resp.msg_seq_num, 1);
}

#[tokio::test]
async fn test_concurrent_connections() {
    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    tokio::spawn(async move {
        server.run().await.ok();
    });

    sleep(Duration::from_millis(100)).await;

    // Connect multiple clients
    let mut handles = vec![];

    for i in 0..3 {
        let addr_clone = addr;
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr_clone).await.unwrap();
            let client = ClientSession::new(test_client_config());

            // Logon
            stream
                .write_all(&client.create_logon_message())
                .await
                .unwrap();
            let mut buffer = vec![0u8; 4096];
            let n = stream.read(&mut buffer).await.unwrap();

            let mut parser = Parser::new();
            parser.feed(&buffer[..n]);
            let logon_resp = parser.parse_message().unwrap().unwrap();

            assert_eq!(logon_resp.msg_type, MsgType::Logon);

            // Send a test order
            let mut order = FixMessage::new(
                MsgType::NewOrderSingle,
                client.current_seq_num_out(),
                "TEST_CLIENT".to_string(),
                "Coinbase".to_string(),
                format!("20260212-12:00:{:02}.000", i),
            );
            order.add_field(Tag::ClOrdID, format!("CONCURRENT_{}", i));
            order.add_field(Tag::Symbol, "BTC-USD".to_string());
            order.add_field(Tag::Side, "1".to_string());
            order.add_field(Tag::OrderQty, "0.1".to_string());
            order.add_field(Tag::OrdType, "2".to_string());
            order.add_field(Tag::Price, "50000.00".to_string());

            stream.write_all(&build_fix_message(&order)).await.unwrap();
            let n = stream.read(&mut buffer).await.unwrap();

            parser.feed(&buffer[..n]);
            let exec_report = parser.parse_message().unwrap().unwrap();

            assert_eq!(exec_report.msg_type, MsgType::ExecutionReport);
        });

        handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_session_with_heartbeats() {
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

    // Send periodic heartbeats
    for _ in 0..3 {
        sleep(Duration::from_millis(500)).await;
        stream
            .write_all(&client.create_heartbeat_message(None))
            .await
            .unwrap();
    }

    // Session should still be alive
    // Send a test request to verify
    stream
        .write_all(&client.create_test_request_message("HB_TEST"))
        .await
        .unwrap();

    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let response = parser.parse_message().unwrap().unwrap();

    assert_eq!(response.msg_type, MsgType::Heartbeat);
    assert_eq!(response.get_field(Tag::TestReqID), Some("HB_TEST"));
}

#[tokio::test]
async fn test_order_execution_with_market_data() {
    // Simulates receiving market data while placing orders
    // This tests that the server can handle both message streams

    let mut server = MockCoinbaseServer::new(test_server_config());
    server.start("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();

    server
        .configure_market_data(vec!["BTC-USD".to_string()])
        .await;

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

    // Request SecurityList
    let mut sec_list_req = FixMessage::new(
        MsgType::SecurityListRequest,
        client.current_seq_num_out(),
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:00.000".to_string(),
    );
    sec_list_req.add_field(Tag::SecurityReqID, "SEC_1".to_string());

    stream
        .write_all(&build_fix_message(&sec_list_req))
        .await
        .unwrap();
    let _ = stream.read(&mut buffer).await.unwrap();

    // Now place an order
    let mut order = FixMessage::new(
        MsgType::NewOrderSingle,
        client.current_seq_num_out() + 1,
        "TEST_CLIENT".to_string(),
        "Coinbase".to_string(),
        "20260212-12:00:01.000".to_string(),
    );
    order.add_field(Tag::ClOrdID, "MD_ORDER_1".to_string());
    order.add_field(Tag::Symbol, "BTC-USD".to_string());
    order.add_field(Tag::Side, "1".to_string());
    order.add_field(Tag::OrderQty, "0.2".to_string());
    order.add_field(Tag::OrdType, "2".to_string());
    order.add_field(Tag::Price, "50000.00".to_string());

    stream.write_all(&build_fix_message(&order)).await.unwrap();
    let n = timeout(Duration::from_secs(1), stream.read(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let mut parser = Parser::new();
    parser.feed(&buffer[..n]);
    let exec_report = parser.parse_message().unwrap().unwrap();

    assert_eq!(exec_report.msg_type, MsgType::ExecutionReport);
    assert_eq!(exec_report.get_field(Tag::ExecType), Some("F"));
}
