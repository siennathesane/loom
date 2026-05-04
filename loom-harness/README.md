# loom-harness

FIX protocol compliance test harness for the Loom cryptocurrency arbitrage engine.

This crate provides a mock Coinbase FIX server for testing the Loom FIX client implementation before connecting to production endpoints.

## Features

- **Mock FIX Server**: Simulates Coinbase's FIX 5.0 SP2 protocol behavior
- **Session Management**: Logon with HMAC validation, heartbeats, sequence tracking
- **Market Data Simulation**: SecurityList, MarketDataRequest, incremental updates
- **Order Execution Simulation**: NewOrderSingle, ExecutionReport, OrderCancelRequest
- **Concurrent Sessions**: Handles multiple client connections simultaneously
- **Comprehensive Testing**: 22 passing test scenarios covering all protocol features

## Quick Start

### Running the Mock Server

```bash
cargo run --example mock_server -p loom-harness
```

This starts a mock FIX server on `localhost:9878` with default credentials:
- API Secret: `test_secret`
- API Passphrase: `test_passphrase`

### Running Tests

```bash
# Run all tests
cargo test -p loom-harness

# Run specific test category
cargo test -p loom-harness --test session_tests
cargo test -p loom-harness --test order_entry_tests
cargo test -p loom-harness --test integration_tests
```

### Using in Code

```rust
use loom_harness::server::{MockCoinbaseServer, MockServerConfig};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Create and configure server
    let mut server = MockCoinbaseServer::new(MockServerConfig::default());
    server.start("127.0.0.1:0").await?;

    // Configure available instruments
    server.configure_market_data(vec![
        "BTC-USD".to_string(),
        "ETH-USD".to_string(),
    ]).await;

    // Run server
    server.run().await
}
```

## Test Coverage

- **Session-Level (7 tests)**: Logon, heartbeats, sequence tracking, logout
- **Market Data (5 tests)**: SecurityList, subscriptions, incremental refresh
- **Order Entry (5 tests)**: Order submission, fills, rejections, cancellations
- **Integration (5 tests)**: Full session lifecycles, reconnection, concurrent clients

See `TEST_COVERAGE.md` for detailed test documentation.

## Architecture

### MockCoinbaseServer

The main server component that:
- Accepts TCP connections on a configurable address
- Manages multiple concurrent FIX sessions
- Delegates to MarketDataSimulator and OrderBookSimulator
- Handles session-level protocol (Logon, Heartbeat, TestRequest, etc.)

### MarketDataSimulator

Simulates market data responses:
- SecurityList with configurable instruments
- MarketDataIncrementalRefresh messages
- SecurityStatus updates

### OrderBookSimulator

Simulates order execution:
- Accepts NewOrderSingle orders
- Generates ExecutionReport messages (filled/rejected)
- Handles OrderCancelRequest
- Configurable fill/reject behavior for testing

## Testing Against Real Coinbase

After validating against this mock server, connect to Coinbase's sandbox environment:

- **Sandbox**: `fix.sandbox.exchange.coinbase.com:4197`
- **Production**: `fix.exchange.coinbase.com:4197`

Both require valid API credentials with appropriate permissions.

## Design Reference

This implementation follows the FIX compliance test framework specification from `design.md` §3.4.

## License

Part of the Loom project. See repository root for license information.
