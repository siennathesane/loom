// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use loom_config::LoomConfig;
use loom_engine::{ListenConfig, ListenEventLoop};
use loom_graph::AdjacencyMatrix;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

#[test]
fn test_config_loading() {
    let mut temp_file = NamedTempFile::new().unwrap();
    let config_content = r#"
[fix]
address = "fix.test.com:4198"
server_name = "fix.test.com"
sender_comp_id = "TEST_KEY"
target_comp_id = "TestExchange"
api_secret = "TEST_SECRET"
passphrase = "TEST_PASS"
heartbeat_interval = 30

[engine]
safety_margin = 1.5
fee_bps = 60
cycle_check_interval_ms = 100

[server]
transport = "unix"
address = "/tmp/loom_test.sock"

[database]
path = "./loom_test.db"
"#;
    temp_file.write_all(config_content.as_bytes()).unwrap();

    let config = LoomConfig::from_file(temp_file.path().to_str().unwrap()).unwrap();
    assert_eq!(config.fix.address, "fix.test.com:4198");
    assert_eq!(config.engine.safety_margin, 1.5);
    assert_eq!(config.server.transport, "unix");
}

#[test]
fn test_example_config_generation() {
    let config = LoomConfig::sandbox_example();
    let toml_str = toml::to_string_pretty(&config).unwrap();

    // Verify it contains required sections
    assert!(toml_str.contains("[fix]"));
    assert!(toml_str.contains("[engine]"));
    assert!(toml_str.contains("[server]"));
    assert!(toml_str.contains("[database]"));

    // Verify Coinbase sandbox endpoint
    assert!(toml_str.contains("fix-public.sandbox.exchange.coinbase.com"));

    // Verify it can be parsed back
    let _parsed: LoomConfig = toml::from_str(&toml_str).unwrap();
}

#[test]
fn test_graph_initialization() {
    let currencies = vec!["BTC".to_string(), "ETH".to_string(), "USD".to_string()];

    let graph = AdjacencyMatrix::new(currencies.clone());
    // Graph should be initialized with the currencies
    // Basic sanity check - graph creation doesn't panic
    drop(graph);
}

#[test]
fn test_event_loop_creation_without_database() {
    let currencies = vec!["BTC".to_string(), "ETH".to_string(), "USD".to_string()];

    let graph = AdjacencyMatrix::new(currencies);
    let config = ListenConfig {
        safety_margin: 1.5,
        fee_bps: 60,
        cycle_check_interval_ms: 100,
    };

    let event_loop = ListenEventLoop::new(graph, config);
    // Event loop should be created successfully
    drop(event_loop);
}

#[test]
fn test_event_loop_creation_with_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let currencies = vec!["BTC".to_string(), "ETH".to_string(), "USD".to_string()];

    let graph = AdjacencyMatrix::new(currencies);
    let config = ListenConfig {
        safety_margin: 1.5,
        fee_bps: 60,
        cycle_check_interval_ms: 100,
    };

    let result = ListenEventLoop::with_database(graph, config, db_path.to_str().unwrap());

    assert!(result.is_ok());
    // Database file should be created
    assert!(db_path.exists());
}

#[test]
fn test_event_loop_with_invalid_database_path() {
    let currencies = vec!["BTC".to_string()];
    let graph = AdjacencyMatrix::new(currencies);
    let config = ListenConfig {
        safety_margin: 1.5,
        fee_bps: 60,
        cycle_check_interval_ms: 100,
    };

    // Try to create database in non-existent directory
    let result = ListenEventLoop::with_database(graph, config, "/nonexistent/directory/test.db");

    assert!(result.is_err());
}

#[test]
fn test_listen_config_default_values() {
    let config = ListenConfig::default();
    assert_eq!(config.safety_margin, 1.5);
    assert_eq!(config.fee_bps, 60);
    assert_eq!(config.cycle_check_interval_ms, 100);
}

#[test]
fn test_listen_config_custom_values() {
    let config = ListenConfig {
        safety_margin: 2.0,
        fee_bps: 50,
        cycle_check_interval_ms: 200,
    };
    assert_eq!(config.safety_margin, 2.0);
    assert_eq!(config.fee_bps, 50);
    assert_eq!(config.cycle_check_interval_ms, 200);
}

#[test]
fn test_config_to_listen_config_conversion() {
    let loom_config = LoomConfig::sandbox_example();
    let listen_config = ListenConfig {
        safety_margin: loom_config.engine.safety_margin,
        fee_bps: loom_config.engine.fee_bps as u16,
        cycle_check_interval_ms: loom_config.engine.cycle_check_interval_ms,
    };

    assert_eq!(listen_config.safety_margin, 1.5);
    assert_eq!(listen_config.fee_bps, 60);
    assert_eq!(listen_config.cycle_check_interval_ms, 100);
}

#[test]
fn test_multiple_currency_graph_initialization() {
    let currencies = vec![
        "BTC".to_string(),
        "ETH".to_string(),
        "USD".to_string(),
        "EUR".to_string(),
        "GBP".to_string(),
    ];

    let graph = AdjacencyMatrix::new(currencies.clone());
    drop(graph);
}

#[test]
fn test_single_currency_graph() {
    let currencies = vec!["USD".to_string()];
    let graph = AdjacencyMatrix::new(currencies);
    drop(graph);
}

#[test]
fn test_empty_currency_list() {
    let currencies: Vec<String> = vec![];
    let graph = AdjacencyMatrix::new(currencies);
    drop(graph);
}

#[test]
fn test_database_path_with_subdirectory() {
    let temp_dir = TempDir::new().unwrap();
    let subdir = temp_dir.path().join("data");
    std::fs::create_dir(&subdir).unwrap();
    let db_path = subdir.join("test.db");

    let currencies = vec!["BTC".to_string(), "USD".to_string()];
    let graph = AdjacencyMatrix::new(currencies);
    let config = ListenConfig::default();

    let result = ListenEventLoop::with_database(graph, config, db_path.to_str().unwrap());

    assert!(result.is_ok());
    assert!(db_path.exists());
}

#[test]
fn test_transport_type_unix() {
    let config = LoomConfig::sandbox_example();
    assert_eq!(config.server.transport, "unix");
    assert!(config.server.address.starts_with("/tmp/"));
}

#[test]
fn test_transport_type_tcp() {
    let mut config = LoomConfig::sandbox_example();
    config.server.transport = "tcp".to_string();
    config.server.address = "127.0.0.1:50051".to_string();

    assert_eq!(config.server.transport, "tcp");
    assert!(config.server.address.contains(":"));
}

#[test]
fn test_fix_config_heartbeat_interval() {
    let config = LoomConfig::sandbox_example();
    assert!(config.fix.heartbeat_interval > 0);
    assert!(config.fix.heartbeat_interval <= 300); // Reasonable upper bound
}

#[test]
fn test_engine_config_safety_margin_range() {
    let config = LoomConfig::sandbox_example();
    assert!(config.engine.safety_margin > 1.0); // Should be > 100%
    assert!(config.engine.safety_margin < 10.0); // Reasonable upper bound
}

#[test]
fn test_engine_config_fee_bps_range() {
    let config = LoomConfig::sandbox_example();
    assert!(config.engine.fee_bps > 0);
    assert!(config.engine.fee_bps < 1000); // Less than 10%
}

#[test]
fn test_cycle_check_interval_range() {
    let config = LoomConfig::sandbox_example();
    assert!(config.engine.cycle_check_interval_ms > 0);
    assert!(config.engine.cycle_check_interval_ms < 10000); // Less than 10 seconds
}

#[test]
fn test_coinbase_sandbox_endpoint_format() {
    let config = LoomConfig::sandbox_example();
    assert!(config.fix.address.contains(":"));
    assert!(config.fix.address.contains("sandbox"));
    assert!(config.fix.address.contains("coinbase"));
}

#[test]
fn test_server_name_matches_address() {
    let config = LoomConfig::sandbox_example();
    // Server name should be the hostname from address
    let hostname = config.fix.address.split(':').next().unwrap();
    assert_eq!(config.fix.server_name, hostname);
}

#[test]
fn test_config_clone() {
    let config1 = LoomConfig::sandbox_example();
    let config2 = config1.clone();

    assert_eq!(config1.fix.address, config2.fix.address);
    assert_eq!(config1.engine.safety_margin, config2.engine.safety_margin);
    assert_eq!(config1.server.transport, config2.server.transport);
}

#[test]
fn test_listen_config_clone() {
    let config1 = ListenConfig::default();
    let config2 = config1.clone();

    assert_eq!(config1.safety_margin, config2.safety_margin);
    assert_eq!(config1.fee_bps, config2.fee_bps);
    assert_eq!(
        config1.cycle_check_interval_ms,
        config2.cycle_check_interval_ms
    );
}

#[test]
fn test_multiple_event_loops_with_different_databases() {
    let temp_dir = TempDir::new().unwrap();
    let db1_path = temp_dir.path().join("test1.db");
    let db2_path = temp_dir.path().join("test2.db");

    let currencies = vec!["BTC".to_string(), "USD".to_string()];
    let config = ListenConfig::default();

    let graph1 = AdjacencyMatrix::new(currencies.clone());
    let result1 =
        ListenEventLoop::with_database(graph1, config.clone(), db1_path.to_str().unwrap());
    assert!(result1.is_ok());

    let graph2 = AdjacencyMatrix::new(currencies);
    let result2 = ListenEventLoop::with_database(graph2, config, db2_path.to_str().unwrap());
    assert!(result2.is_ok());

    assert!(db1_path.exists());
    assert!(db2_path.exists());
}

#[test]
fn test_config_debug_format() {
    let config = LoomConfig::sandbox_example();
    let debug_str = format!("{:?}", config);
    assert!(!debug_str.is_empty());
    assert!(debug_str.contains("LoomConfig"));
}

#[test]
fn test_listen_config_debug_format() {
    let config = ListenConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(!debug_str.is_empty());
    assert!(debug_str.contains("ListenConfig"));
}
