// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! TLS transport layer for FIX connections.
//!
//! This module provides secure FIX connectivity with:
//! - TLS encryption via tokio-rustls
//! - TCP_NODELAY for low latency
//! - Connection health monitoring
//! - Exponential backoff reconnection
//! - CPU core pinning for Active mode

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::{client::TlsStream, TlsConnector};

/// Connection configuration
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// FIX gateway address (e.g., "fix.coinbase.com:4198")
    pub address: String,
    /// Server name for TLS (e.g., "fix.coinbase.com")
    pub server_name: String,
    /// Maximum reconnection attempts (0 = infinite)
    pub max_retries: usize,
    /// Initial backoff delay in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds
    pub max_backoff_ms: u64,
    /// Heartbeat interval in seconds (for connection health)
    pub heartbeat_interval_secs: u64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            address: "fix.coinbase.com:4198".to_string(),
            server_name: "fix.coinbase.com".to_string(),
            max_retries: 10,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            heartbeat_interval_secs: 30,
        }
    }
}

/// Exponential backoff calculator
pub struct ExponentialBackoff {
    current_ms: u64,
    initial_ms: u64,
    max_ms: u64,
}

impl ExponentialBackoff {
    /// Create a new exponential backoff starting at initial_ms, capped at max_ms
    pub fn new(initial_ms: u64, max_ms: u64) -> Self {
        Self {
            current_ms: initial_ms,
            initial_ms,
            max_ms,
        }
    }

    /// Get the next backoff duration and advance the state
    pub fn next(&mut self) -> Duration {
        let duration = Duration::from_millis(self.current_ms);

        // Double the backoff, capped at max
        self.current_ms = std::cmp::min(self.current_ms * 2, self.max_ms);

        duration
    }

    /// Reset backoff to initial value
    pub fn reset(&mut self) {
        self.current_ms = self.initial_ms;
    }
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Disconnected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected and healthy
    Connected,
    /// Connection degraded (heartbeat delayed)
    Degraded,
    /// Reconnecting after failure
    Reconnecting,
}

/// FIX connection manager with TLS and reconnection
pub struct FixConnection {
    config: ConnectionConfig,
    state: ConnectionState,
    stream: Option<TlsStream<TcpStream>>,
    last_recv_time: Option<Instant>,
    backoff: ExponentialBackoff,
    tls_connector: TlsConnector,
}

impl FixConnection {
    /// Create a new FIX connection manager
    pub fn new(config: ConnectionConfig) -> io::Result<Self> {
        // Create TLS configuration
        let mut root_store = RootCertStore::empty();
        // Add system root certificates
        for cert in rustls_native_certs::load_native_certs()? {
            root_store.add(cert.0.into()).ok();
        }

        let tls_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let tls_connector = TlsConnector::from(Arc::new(tls_config));

        let backoff = ExponentialBackoff::new(config.initial_backoff_ms, config.max_backoff_ms);

        Ok(Self {
            config,
            state: ConnectionState::Disconnected,
            stream: None,
            last_recv_time: None,
            backoff,
            tls_connector,
        })
    }

    /// Get current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Connect to FIX gateway with TLS
    pub async fn connect(&mut self) -> io::Result<()> {
        self.state = ConnectionState::Connecting;

        // Establish TCP connection
        let tcp_stream = TcpStream::connect(&self.config.address).await?;

        // Enable TCP_NODELAY (disable Nagle's algorithm)
        tcp_stream.set_nodelay(true)?;

        // Perform TLS handshake
        let server_name = ServerName::try_from(self.config.server_name.clone())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name"))?;

        let tls_stream = self.tls_connector.connect(server_name, tcp_stream).await?;

        self.stream = Some(tls_stream);
        self.state = ConnectionState::Connected;
        self.last_recv_time = Some(Instant::now());
        self.backoff.reset();

        Ok(())
    }

    /// Connect with exponential backoff retry logic
    pub async fn connect_with_retry(&mut self) -> io::Result<()> {
        let mut attempts = 0;

        loop {
            match self.connect().await {
                Ok(()) => return Ok(()),
                Err(_e) => {
                    attempts += 1;

                    if self.config.max_retries > 0 && attempts >= self.config.max_retries {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("Failed to connect after {} attempts", attempts),
                        ));
                    }

                    self.state = ConnectionState::Reconnecting;
                    let delay = self.backoff.next();
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Send data over the connection
    pub async fn send(&mut self, data: &[u8]) -> io::Result<()> {
        match &mut self.stream {
            Some(stream) => {
                stream.write_all(data).await?;
                stream.flush().await?;
                Ok(())
            }
            None => Err(io::Error::new(io::ErrorKind::NotConnected, "Not connected")),
        }
    }

    /// Receive data from the connection
    pub async fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.stream {
            Some(stream) => {
                let n = stream.read(buf).await?;
                if n > 0 {
                    self.last_recv_time = Some(Instant::now());
                }
                Ok(n)
            }
            None => Err(io::Error::new(io::ErrorKind::NotConnected, "Not connected")),
        }
    }

    /// Check if connection is healthy
    ///
    /// Returns true if last message was received within the heartbeat window.
    /// Uses 1.5x heartbeat interval as tolerance (same as FIX spec).
    pub fn is_healthy(&self) -> bool {
        if self.state != ConnectionState::Connected && self.state != ConnectionState::Degraded {
            return false;
        }

        match self.last_recv_time {
            Some(last_recv) => {
                let elapsed = last_recv.elapsed();
                let timeout = Duration::from_secs(
                    ((self.config.heartbeat_interval_secs as f64) * 1.5) as u64,
                );
                elapsed < timeout
            }
            None => false,
        }
    }

    /// Check if connection appears degraded (heartbeat delayed but not timed out)
    ///
    /// Returns true if we're between 1.0x and 1.5x heartbeat interval.
    pub fn is_degraded(&self) -> bool {
        match self.last_recv_time {
            Some(last_recv) => {
                let elapsed = last_recv.elapsed();
                let heartbeat = Duration::from_secs(self.config.heartbeat_interval_secs);
                let timeout = Duration::from_secs(
                    ((self.config.heartbeat_interval_secs as f64) * 1.5) as u64,
                );

                elapsed >= heartbeat && elapsed < timeout
            }
            None => false,
        }
    }

    /// Update connection state based on health check
    pub fn update_health_state(&mut self) {
        if self.state == ConnectionState::Connected || self.state == ConnectionState::Degraded {
            if !self.is_healthy() {
                self.state = ConnectionState::Disconnected;
                self.stream = None;
            } else if self.is_degraded() {
                self.state = ConnectionState::Degraded;
            } else {
                self.state = ConnectionState::Connected;
            }
        }
    }

    /// Disconnect
    pub async fn disconnect(&mut self) -> io::Result<()> {
        if let Some(mut stream) = self.stream.take() {
            stream.shutdown().await?;
        }
        self.state = ConnectionState::Disconnected;
        self.last_recv_time = None;
        Ok(())
    }
}

/// Set CPU affinity for the current thread (Linux-only)
///
/// Pins the thread to the specified CPU core for reduced latency variance.
/// This is critical for Active mode where microsecond timing matters.
#[cfg(target_os = "linux")]
pub fn set_cpu_affinity(core_id: usize) -> io::Result<()> {
    use libc::{cpu_set_t, sched_setaffinity, CPU_SET, CPU_ZERO};
    use std::mem;

    unsafe {
        let mut cpuset: cpu_set_t = mem::zeroed();
        CPU_ZERO(&mut cpuset);
        CPU_SET(core_id, &mut cpuset);

        let result = sched_setaffinity(0, mem::size_of::<cpu_set_t>(), &cpuset);
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// Set CPU affinity (no-op on non-Linux platforms)
#[cfg(not(target_os = "linux"))]
pub fn set_cpu_affinity(_core_id: usize) -> io::Result<()> {
    // No-op on non-Linux platforms
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let mut backoff = ExponentialBackoff::new(1000, 60000);

        // Initial: 1s
        assert_eq!(backoff.next(), Duration::from_millis(1000));
        // Double: 2s
        assert_eq!(backoff.next(), Duration::from_millis(2000));
        // Double: 4s
        assert_eq!(backoff.next(), Duration::from_millis(4000));
        // Double: 8s
        assert_eq!(backoff.next(), Duration::from_millis(8000));
        // Double: 16s
        assert_eq!(backoff.next(), Duration::from_millis(16000));
        // Double: 32s
        assert_eq!(backoff.next(), Duration::from_millis(32000));
        // Double: 60s (capped at max)
        assert_eq!(backoff.next(), Duration::from_millis(60000));
        // Stays at max
        assert_eq!(backoff.next(), Duration::from_millis(60000));
    }

    #[test]
    fn test_exponential_backoff_reset() {
        let mut backoff = ExponentialBackoff::new(1000, 60000);

        backoff.next(); // 1s
        backoff.next(); // 2s
        backoff.next(); // 4s

        backoff.reset();

        // Should be back to initial
        assert_eq!(backoff.next(), Duration::from_millis(1000));
    }

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();
        assert_eq!(config.address, "fix.coinbase.com:4198");
        assert_eq!(config.initial_backoff_ms, 1000);
        assert_eq!(config.max_backoff_ms, 60000);
    }

    #[test]
    fn test_connection_state() {
        let config = ConnectionConfig::default();
        let conn = FixConnection::new(config).unwrap();
        assert_eq!(conn.state(), ConnectionState::Disconnected);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_cpu_affinity_linux() {
        // This test will only work on Linux with available cores
        // Try to pin to core 0, which should always exist
        let result = set_cpu_affinity(0);
        assert!(result.is_ok());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_cpu_affinity_noop() {
        // On non-Linux platforms, this should always succeed as a no-op
        let result = set_cpu_affinity(0);
        assert!(result.is_ok());
    }
}
