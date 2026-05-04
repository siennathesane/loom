// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Mock Coinbase FIX server implementation.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::session::server::{ServerConfig, ServerSession, ServerSessionState};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

use crate::market_data::MarketDataSimulator;
use crate::order_book::OrderBookSimulator;

/// Configuration for the mock server
#[derive(Debug, Clone)]
pub struct MockServerConfig {
    /// Server CompID (usually "Coinbase")
    pub server_comp_id: String,
    /// Expected API secret for HMAC validation
    pub api_secret: String,
    /// Expected API passphrase
    pub api_passphrase: String,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u32,
}

impl Default for MockServerConfig {
    fn default() -> Self {
        Self {
            server_comp_id: "Coinbase".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            heartbeat_interval: 30,
        }
    }
}

impl From<MockServerConfig> for ServerConfig {
    fn from(config: MockServerConfig) -> Self {
        ServerConfig {
            server_comp_id: config.server_comp_id,
            api_secret: config.api_secret,
            api_passphrase: config.api_passphrase,
            heartbeat_interval: config.heartbeat_interval,
        }
    }
}

/// Mock Coinbase FIX server
pub struct MockCoinbaseServer {
    config: MockServerConfig,
    sessions: Arc<RwLock<HashMap<SocketAddr, ServerSession>>>,
    market_data: Arc<RwLock<MarketDataSimulator>>,
    pub order_book: Arc<RwLock<OrderBookSimulator>>,
    listener: Option<TcpListener>,
    local_addr: Option<SocketAddr>,
}

impl MockCoinbaseServer {
    /// Create a new mock server
    pub fn new(config: MockServerConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            market_data: Arc::new(RwLock::new(MarketDataSimulator::new())),
            order_book: Arc::new(RwLock::new(OrderBookSimulator::new())),
            listener: None,
            local_addr: None,
        }
    }

    /// Get the local address the server is bound to
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    /// Start listening for connections
    pub async fn start(&mut self, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        self.local_addr = Some(listener.local_addr()?);
        self.listener = Some(listener);
        Ok(())
    }

    /// Run the server (accepts connections and handles them)
    pub async fn run(&mut self) -> std::io::Result<()> {
        let listener = self.listener.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "Server not started")
        })?;

        loop {
            let (socket, addr) = listener.accept().await?;

            let session = ServerSession::new(self.config.clone().into());
            {
                let mut sessions = self.sessions.write().await;
                sessions.insert(addr, session);
            }

            let sessions = Arc::clone(&self.sessions);
            let market_data = Arc::clone(&self.market_data);
            let order_book = Arc::clone(&self.order_book);
            let heartbeat_interval = self.config.heartbeat_interval;

            tokio::spawn(async move {
                if let Err(e) = handle_connection(
                    socket,
                    addr,
                    sessions,
                    market_data,
                    order_book,
                    heartbeat_interval,
                )
                .await
                {
                    eprintln!("Connection error for {}: {}", addr, e);
                }
            });
        }
    }

    /// Configure market data simulator
    pub async fn configure_market_data(&self, instruments: Vec<String>) {
        let mut md = self.market_data.write().await;
        md.set_instruments(instruments);
    }

    /// Simulate an order fill for testing
    pub async fn simulate_order_fill(&self, order_id: &str, filled: bool) {
        let mut ob = self.order_book.write().await;
        ob.simulate_fill(order_id, filled);
    }

    /// Trigger market data updates
    pub async fn send_market_data_update(&self, instrument: &str, price: f64, size: f64) {
        let mut md = self.market_data.write().await;
        md.add_update(instrument.to_string(), price, size);
    }
}

/// Handle a single client connection
async fn handle_connection(
    mut socket: TcpStream,
    addr: SocketAddr,
    sessions: Arc<RwLock<HashMap<SocketAddr, ServerSession>>>,
    market_data: Arc<RwLock<MarketDataSimulator>>,
    order_book: Arc<RwLock<OrderBookSimulator>>,
    heartbeat_interval: u32,
) -> std::io::Result<()> {
    let mut buffer = vec![0u8; 8192];
    let mut heartbeat_check = interval(Duration::from_secs(heartbeat_interval as u64));

    loop {
        tokio::select! {
            // Read data from client
            result = socket.read(&mut buffer) => {
                let n = result?;
                if n == 0 {
                    // Connection closed
                    let mut sessions_guard = sessions.write().await;
                    sessions_guard.remove(&addr);
                    return Ok(());
                }

                // Feed data to session parser
                let mut sessions_guard = sessions.write().await;
                if let Some(session) = sessions_guard.get_mut(&addr) {
                    session.feed_data(&buffer[..n]);

                    // Process all available messages
                    loop {
                        let msg = match session.parse_next_message() {
                            Ok(Some(m)) => m,
                            Ok(None) => break,
                            Err(_e) => {
                                // Parse error - close connection
                                sessions_guard.remove(&addr);
                                return Ok(());
                            }
                        };
                        let response = handle_message(
                            session,
                            msg,
                            &market_data,
                            &order_book,
                        ).await?;

                        if let Some(resp_bytes) = response {
                            socket.write_all(&resp_bytes).await?;
                        }

                        // Check if we should close connection
                        if session.state() == ServerSessionState::Disconnected {
                            sessions_guard.remove(&addr);
                            return Ok(());
                        }
                    } // end loop
                }
            }

            // Check for heartbeat timeout
            _ = heartbeat_check.tick() => {
                let mut sessions_guard = sessions.write().await;
                if let Some(session) = sessions_guard.get_mut(&addr) {
                    if session.check_heartbeat_timeout() {
                        // Send TestRequest
                        let test_req = session.create_test_request_message();
                        socket.write_all(&test_req).await?;
                    }
                }
            }
        }
    }
}

/// Handle a single FIX message
async fn handle_message(
    session: &mut ServerSession,
    msg: FixMessage,
    market_data: &Arc<RwLock<MarketDataSimulator>>,
    order_book: &Arc<RwLock<OrderBookSimulator>>,
) -> std::io::Result<Option<Vec<u8>>> {
    match msg.msg_type {
        MsgType::Logon => {
            // Validate and respond to Logon
            if session.validate_logon(&msg).is_ok() {
                session.set_state(ServerSessionState::LoggedIn);
                session.increment_seq_num_in();
                Ok(Some(session.create_logon_response()))
            } else {
                // Validation failed - just close the connection
                session.set_state(ServerSessionState::Disconnected);
                Ok(None)
            }
        }

        MsgType::Heartbeat => {
            // Acknowledge heartbeat
            session.increment_seq_num_in();
            Ok(None)
        }

        MsgType::TestRequest => {
            // Respond with Heartbeat containing TestReqID
            session.increment_seq_num_in();
            let test_req_id = msg.get_field(Tag::TestReqID);
            Ok(Some(session.create_heartbeat_message(test_req_id)))
        }

        MsgType::SecurityListRequest => {
            session.increment_seq_num_in();
            let md = market_data.read().await;
            Ok(Some(md.create_security_list_response(session)))
        }

        MsgType::MarketDataRequest => {
            session.increment_seq_num_in();
            // For now, just acknowledge - real impl would track subscriptions
            Ok(None)
        }

        MsgType::NewOrderSingle => {
            session.increment_seq_num_in();
            let mut ob = order_book.write().await;
            Ok(Some(ob.handle_new_order(session, &msg)))
        }

        MsgType::OrderCancelRequest => {
            session.increment_seq_num_in();
            let mut ob = order_book.write().await;
            Ok(Some(ob.handle_cancel_request(session, &msg)))
        }

        MsgType::Logout => {
            session.increment_seq_num_in();
            session.set_state(ServerSessionState::LoggingOut);
            let logout = session.create_logout_message();
            session.set_state(ServerSessionState::Disconnected);
            Ok(Some(logout))
        }

        MsgType::ResendRequest => {
            session.increment_seq_num_in();
            // Send SequenceReset-GapFill
            let new_seq_no = session.current_seq_num_out();
            Ok(Some(session.create_sequence_reset(new_seq_no)))
        }

        _ => {
            // Unknown message type - ignore
            session.increment_seq_num_in();
            Ok(None)
        }
    }
}
