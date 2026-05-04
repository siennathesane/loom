# loom-fix

Hand-rolled FIX 5.0 SP2 protocol implementation optimized for zero-allocation, high-performance communication with Coinbase's FIX trading infrastructure. This crate handles all protocol-level concerns: parsing, serialization, session management, TLS transport, and HMAC authentication.

## Overview

`loom-fix` is a custom FIX implementation focused on the specific message types and fields required for market data subscriptions and order submission on Coinbase. It avoids the complexity of full FIX stacks while maintaining correctness for the subset of FIX 5.0 SP2 used in cryptocurrency arbitrage. Features include zero-copy parsing, state machine-based session management, and TLS with certificate pinning.

## Public API

- `FixMessage` — Parsed FIX message with tag-value pairs
- `Parser` — State machine that consumes byte streams and emits complete messages
- `Serializer` / `serialize_message()` — Zero-allocation message encoding
- `FixConnection` — TLS transport with automatic reconnection and backoff
- `ClientSession` — Client-side session state machine with HMAC signing
- `MarketDataIncrementalRefresh`, `SecurityList` — Parsed order book and instrument data

## Usage

```rust
use loom_fix::transport::{FixConnection, ConnectionConfig};
use loom_fix::session::client::{ClientSession, SessionConfig};

// Configure and connect
let config = ConnectionConfig {
    address: "fix-public.sandbox.exchange.coinbase.com:4198".into(),
    server_name: "fix-public.sandbox.exchange.coinbase.com".into(),
    max_retries: 10,
    initial_backoff_ms: 1000,
    max_backoff_ms: 60000,
    heartbeat_interval_secs: 30,
};

let mut conn = FixConnection::new(config)?;
conn.connect().await?;

// Create a session with API credentials
let session_config = SessionConfig {
    sender_comp_id: "YOUR_API_KEY".into(),
    target_comp_id: "Coinbase".into(),
    api_secret: "YOUR_SECRET".into(),
    api_passphrase: "YOUR_PASS".into(),
    heartbeat_interval: 30,
};

let mut session = ClientSession::new(session_config);
let logon_msg = session.create_logon_message();
conn.send(&logon_msg).await?;

// Receive and parse messages
let mut buf = vec![0u8; 8192];
let n = conn.recv(&mut buf).await?;
session.feed_data(&buf[..n]);
while let Ok(Some(msg)) = session.parse_next_message() {
    println!("Received: {:?}", msg.msg_type);
}
```

## Dependencies on other Loom crates

None — `loom-fix` is self-contained and provides the foundation for FIX integration.

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
