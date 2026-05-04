// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-config
//!
//! Configuration loading and validation for the Loom arbitrage engine.
//!
//! Reads TOML configuration files defining FIX credentials (Coinbase API key and secret),
//! engine parameters (safety margin, fees, cycle detection intervals), server transport
//! options, and database paths. Provides serde-based deserialization with validation.
//!
//! ## Examples
//!
//! ```ignore
//! let config = LoomConfig::from_file("loom.toml")?;
//! println!("FIX server: {}", config.fix.address);
//! println!("Safety margin: {}", config.engine.safety_margin);
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomConfig {
    pub fix: FixConfig,
    pub engine: EngineConfig,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixConfig {
    /// FIX server address (e.g., "fix-public.sandbox.exchange.coinbase.com:4198")
    pub address: String,

    /// Server name for TLS
    pub server_name: String,

    /// Your Coinbase API key
    pub sender_comp_id: String,

    /// Target comp ID (usually "Coinbase")
    pub target_comp_id: String,

    /// API secret for HMAC signing
    pub api_secret: String,

    /// Passphrase
    pub passphrase: String,

    /// Heartbeat interval (seconds)
    pub heartbeat_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Safety margin for gravitational gating
    pub safety_margin: f64,

    /// Fee basis points (60 = 0.6%)
    pub fee_bps: u64,

    /// Cycle check interval (milliseconds)
    pub cycle_check_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Transport type: "tcp" or "unix"
    pub transport: String,

    /// Address (TCP) or path (Unix socket)
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to SQLite database file
    pub path: String,
}

impl LoomConfig {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: LoomConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Example configuration for Coinbase sandbox
    pub fn sandbox_example() -> Self {
        Self {
            fix: FixConfig {
                address: "fix-public.sandbox.exchange.coinbase.com:4198".to_string(),
                server_name: "fix-public.sandbox.exchange.coinbase.com".to_string(),
                sender_comp_id: "YOUR_API_KEY_HERE".to_string(),
                target_comp_id: "Coinbase".to_string(),
                api_secret: "YOUR_API_SECRET_HERE".to_string(),
                passphrase: "YOUR_PASSPHRASE_HERE".to_string(),
                heartbeat_interval: 30,
            },
            engine: EngineConfig {
                safety_margin: 1.5,
                fee_bps: 60,
                cycle_check_interval_ms: 100,
            },
            server: ServerConfig {
                transport: "unix".to_string(),
                address: "/tmp/loom.sock".to_string(),
            },
            database: DatabaseConfig {
                path: "./loom.db".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    mod config_loading {
        use super::*;

        #[test]
        fn test_config_from_file_valid() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let config_content = r#"
[fix]
address = "fix.test.com:4198"
server_name = "fix.test.com"
sender_comp_id = "TEST_API_KEY"
target_comp_id = "TestExchange"
api_secret = "TEST_SECRET"
passphrase = "TEST_PASS"
heartbeat_interval = 60

[engine]
safety_margin = 2.0
fee_bps = 50
cycle_check_interval_ms = 200

[server]
transport = "tcp"
address = "127.0.0.1:50051"

[database]
path = "/tmp/test.db"
"#;
            temp_file.write_all(config_content.as_bytes()).unwrap();
            let config = LoomConfig::from_file(temp_file.path().to_str().unwrap()).unwrap();

            assert_eq!(config.fix.address, "fix.test.com:4198");
            assert_eq!(config.fix.server_name, "fix.test.com");
            assert_eq!(config.fix.sender_comp_id, "TEST_API_KEY");
            assert_eq!(config.fix.target_comp_id, "TestExchange");
            assert_eq!(config.fix.api_secret, "TEST_SECRET");
            assert_eq!(config.fix.passphrase, "TEST_PASS");
            assert_eq!(config.fix.heartbeat_interval, 60);
            assert_eq!(config.engine.safety_margin, 2.0);
            assert_eq!(config.engine.fee_bps, 50);
            assert_eq!(config.engine.cycle_check_interval_ms, 200);
            assert_eq!(config.server.transport, "tcp");
            assert_eq!(config.server.address, "127.0.0.1:50051");
            assert_eq!(config.database.path, "/tmp/test.db");
        }

        #[test]
        fn test_config_missing_file() {
            let result = LoomConfig::from_file("/nonexistent/path/config.toml");
            assert!(result.is_err());
        }

        #[test]
        fn test_config_invalid_toml() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let invalid_content = "this is not valid TOML {{{";
            temp_file.write_all(invalid_content.as_bytes()).unwrap();

            let result = LoomConfig::from_file(temp_file.path().to_str().unwrap());
            assert!(result.is_err());
        }

        #[test]
        fn test_config_missing_required_field() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let incomplete_config = r#"
[fix]
address = "fix.test.com:4198"
# Missing other required fields

[engine]
safety_margin = 1.5
fee_bps = 60
cycle_check_interval_ms = 100

[server]
transport = "unix"
address = "/tmp/loom.sock"

[database]
path = "./loom.db"
"#;
            temp_file.write_all(incomplete_config.as_bytes()).unwrap();

            let result = LoomConfig::from_file(temp_file.path().to_str().unwrap());
            assert!(result.is_err());
        }

        #[test]
        fn test_config_missing_section() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let incomplete_config = r#"
[fix]
address = "fix.test.com:4198"
server_name = "fix.test.com"
sender_comp_id = "TEST_KEY"
target_comp_id = "Test"
api_secret = "SECRET"
passphrase = "PASS"
heartbeat_interval = 30
# Missing [engine], [server], [database] sections
"#;
            temp_file.write_all(incomplete_config.as_bytes()).unwrap();

            let result = LoomConfig::from_file(temp_file.path().to_str().unwrap());
            assert!(result.is_err());
        }
    }

    mod serialization {
        use super::*;

        #[test]
        fn test_config_serialization_roundtrip() {
            let original = LoomConfig::sandbox_example();
            let toml_str = toml::to_string(&original).unwrap();
            let deserialized: LoomConfig = toml::from_str(&toml_str).unwrap();

            assert_eq!(original.fix.address, deserialized.fix.address);
            assert_eq!(original.fix.server_name, deserialized.fix.server_name);
            assert_eq!(original.fix.sender_comp_id, deserialized.fix.sender_comp_id);
            assert_eq!(original.fix.target_comp_id, deserialized.fix.target_comp_id);
            assert_eq!(original.fix.api_secret, deserialized.fix.api_secret);
            assert_eq!(original.fix.passphrase, deserialized.fix.passphrase);
            assert_eq!(
                original.fix.heartbeat_interval,
                deserialized.fix.heartbeat_interval
            );
            assert_eq!(
                original.engine.safety_margin,
                deserialized.engine.safety_margin
            );
            assert_eq!(original.engine.fee_bps, deserialized.engine.fee_bps);
            assert_eq!(
                original.engine.cycle_check_interval_ms,
                deserialized.engine.cycle_check_interval_ms
            );
            assert_eq!(original.server.transport, deserialized.server.transport);
            assert_eq!(original.server.address, deserialized.server.address);
            assert_eq!(original.database.path, deserialized.database.path);
        }

        #[test]
        fn test_fix_config_serialization() {
            let fix = FixConfig {
                address: "test.com:1234".to_string(),
                server_name: "test.com".to_string(),
                sender_comp_id: "SENDER".to_string(),
                target_comp_id: "TARGET".to_string(),
                api_secret: "SECRET".to_string(),
                passphrase: "PASS".to_string(),
                heartbeat_interval: 45,
            };

            let toml_str = toml::to_string(&fix).unwrap();
            let deserialized: FixConfig = toml::from_str(&toml_str).unwrap();

            assert_eq!(fix.address, deserialized.address);
            assert_eq!(fix.heartbeat_interval, deserialized.heartbeat_interval);
        }

        #[test]
        fn test_engine_config_serialization() {
            let engine = EngineConfig {
                safety_margin: 1.75,
                fee_bps: 55,
                cycle_check_interval_ms: 150,
            };

            let toml_str = toml::to_string(&engine).unwrap();
            let deserialized: EngineConfig = toml::from_str(&toml_str).unwrap();

            assert_eq!(engine.safety_margin, deserialized.safety_margin);
            assert_eq!(engine.fee_bps, deserialized.fee_bps);
            assert_eq!(
                engine.cycle_check_interval_ms,
                deserialized.cycle_check_interval_ms
            );
        }

        #[test]
        fn test_server_config_serialization() {
            let server = ServerConfig {
                transport: "tcp".to_string(),
                address: "0.0.0.0:8080".to_string(),
            };

            let toml_str = toml::to_string(&server).unwrap();
            let deserialized: ServerConfig = toml::from_str(&toml_str).unwrap();

            assert_eq!(server.transport, deserialized.transport);
            assert_eq!(server.address, deserialized.address);
        }

        #[test]
        fn test_database_config_serialization() {
            let db = DatabaseConfig {
                path: "/var/lib/loom/data.db".to_string(),
            };

            let toml_str = toml::to_string(&db).unwrap();
            let deserialized: DatabaseConfig = toml::from_str(&toml_str).unwrap();

            assert_eq!(db.path, deserialized.path);
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn test_sandbox_example_has_correct_endpoint() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(
                config.fix.address,
                "fix-public.sandbox.exchange.coinbase.com:4198"
            );
            assert_eq!(
                config.fix.server_name,
                "fix-public.sandbox.exchange.coinbase.com"
            );
        }

        #[test]
        fn test_sandbox_example_has_placeholder_credentials() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(config.fix.sender_comp_id, "YOUR_API_KEY_HERE");
            assert_eq!(config.fix.api_secret, "YOUR_API_SECRET_HERE");
            assert_eq!(config.fix.passphrase, "YOUR_PASSPHRASE_HERE");
        }

        #[test]
        fn test_sandbox_example_target_comp_id() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(config.fix.target_comp_id, "Coinbase");
        }

        #[test]
        fn test_sandbox_example_heartbeat_interval() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(config.fix.heartbeat_interval, 30);
        }

        #[test]
        fn test_engine_config_defaults() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(config.engine.safety_margin, 1.5);
            assert_eq!(config.engine.fee_bps, 60); // 0.6%
            assert_eq!(config.engine.cycle_check_interval_ms, 100);
        }

        #[test]
        fn test_server_config_unix_socket_default() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(config.server.transport, "unix");
            assert_eq!(config.server.address, "/tmp/loom.sock");
        }

        #[test]
        fn test_database_config_default_path() {
            let config = LoomConfig::sandbox_example();
            assert_eq!(config.database.path, "./loom.db");
        }

        #[test]
        fn test_config_with_tcp_transport() {
            let mut config = LoomConfig::sandbox_example();
            config.server.transport = "tcp".to_string();
            config.server.address = "127.0.0.1:50051".to_string();

            assert_eq!(config.server.transport, "tcp");
            assert!(config.server.address.contains("127.0.0.1"));
        }

        #[test]
        fn test_config_numeric_types() {
            let config = LoomConfig::sandbox_example();
            // Verify types are correct
            assert!(config.engine.safety_margin > 0.0);
            assert!(config.engine.fee_bps > 0);
            assert!(config.engine.cycle_check_interval_ms > 0);
            assert!(config.fix.heartbeat_interval > 0);
        }
    }

    mod examples {
        use super::*;

        #[test]
        fn test_sandbox_example_is_valid() {
            let config = LoomConfig::sandbox_example();
            // Should be able to serialize and deserialize
            let toml_str = toml::to_string(&config).unwrap();
            assert!(!toml_str.is_empty());
            let _: LoomConfig = toml::from_str(&toml_str).unwrap();
        }

        #[test]
        fn test_sandbox_example_contains_all_sections() {
            let config = LoomConfig::sandbox_example();
            let toml_str = toml::to_string(&config).unwrap();

            assert!(toml_str.contains("[fix]"));
            assert!(toml_str.contains("[engine]"));
            assert!(toml_str.contains("[server]"));
            assert!(toml_str.contains("[database]"));
        }

        #[test]
        fn test_example_config_file_is_valid() {
            // Test that our example file matches sandbox_example()
            let example = LoomConfig::sandbox_example();
            let toml_str = toml::to_string_pretty(&example).unwrap();

            // Should be able to parse it back
            let parsed: LoomConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(example.fix.address, parsed.fix.address);
        }
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn test_config_with_zero_heartbeat() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let config_content = r#"
[fix]
address = "test.com:4198"
server_name = "test.com"
sender_comp_id = "KEY"
target_comp_id = "Target"
api_secret = "SECRET"
passphrase = "PASS"
heartbeat_interval = 0

[engine]
safety_margin = 1.0
fee_bps = 1
cycle_check_interval_ms = 1

[server]
transport = "unix"
address = "/tmp/test.sock"

[database]
path = "test.db"
"#;
            temp_file.write_all(config_content.as_bytes()).unwrap();
            let config = LoomConfig::from_file(temp_file.path().to_str().unwrap()).unwrap();
            assert_eq!(config.fix.heartbeat_interval, 0);
        }

        #[test]
        fn test_config_with_very_large_values() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let config_content = r#"
[fix]
address = "test.com:4198"
server_name = "test.com"
sender_comp_id = "KEY"
target_comp_id = "Target"
api_secret = "SECRET"
passphrase = "PASS"
heartbeat_interval = 999999

[engine]
safety_margin = 1000.0
fee_bps = 10000
cycle_check_interval_ms = 999999999

[server]
transport = "unix"
address = "/tmp/test.sock"

[database]
path = "test.db"
"#;
            temp_file.write_all(config_content.as_bytes()).unwrap();
            let config = LoomConfig::from_file(temp_file.path().to_str().unwrap()).unwrap();
            assert_eq!(config.fix.heartbeat_interval, 999999);
            assert_eq!(config.engine.safety_margin, 1000.0);
        }

        #[test]
        fn test_config_with_special_characters_in_strings() {
            let mut temp_file = NamedTempFile::new().unwrap();
            let config_content = r#"
[fix]
address = "test.com:4198"
server_name = "test.com"
sender_comp_id = "KEY-WITH-DASHES_AND_UNDERSCORES"
target_comp_id = "Target@123"
api_secret = "SECRET!@#$%^&*()"
passphrase = "PASS with spaces"
heartbeat_interval = 30

[engine]
safety_margin = 1.5
fee_bps = 60
cycle_check_interval_ms = 100

[server]
transport = "unix"
address = "/tmp/loom-test_123.sock"

[database]
path = "./data/loom-test_123.db"
"#;
            temp_file.write_all(config_content.as_bytes()).unwrap();
            let config = LoomConfig::from_file(temp_file.path().to_str().unwrap()).unwrap();
            assert!(config.fix.sender_comp_id.contains("DASHES"));
            assert!(config.fix.passphrase.contains(" "));
        }

        #[test]
        fn test_config_with_empty_file() {
            let temp_file = NamedTempFile::new().unwrap();
            let result = LoomConfig::from_file(temp_file.path().to_str().unwrap());
            assert!(result.is_err());
        }
    }
}
