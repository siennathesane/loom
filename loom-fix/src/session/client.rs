// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Client-side FIX session implementation.
//!
//! Implements the FIX session protocol for client connections:
//! - Logon with HMAC-SHA256 authentication
//! - Heartbeat management (send when idle > HeartBtInt × 0.75)
//! - TestRequest/Heartbeat handling
//! - Sequence number management
//! - ResendRequest for gap recovery
//! - Logout handling

use crate::message::{FixMessage, MsgType, Tag};
use crate::parser::{ParseError, Parser};
use crate::serialization::build_fix_message;
use crate::session::auth::compute_logon_hmac;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{interval, Interval};

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Not connected
    Disconnected,
    /// Connection established, attempting logon
    LoggingIn,
    /// Logged in, session active
    LoggedIn,
    /// Logout sent, awaiting confirmation
    LoggingOut,
}

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// SenderCompID (usually API key)
    pub sender_comp_id: String,
    /// TargetCompID (usually "Coinbase")
    pub target_comp_id: String,
    /// API secret for HMAC
    pub api_secret: String,
    /// API passphrase
    pub api_passphrase: String,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u32,
}

/// FIX client session
pub struct ClientSession {
    /// Session configuration
    config: SessionConfig,
    /// Current session state
    state: SessionState,
    /// Outgoing message sequence number
    msg_seq_num_out: Arc<AtomicU64>,
    /// Expected incoming message sequence number
    msg_seq_num_in: Arc<AtomicU64>,
    /// Parser for incoming messages
    parser: Parser,
    /// Heartbeat interval timer
    heartbeat_timer: Option<Interval>,
    /// Last message send time
    last_send_time: SystemTime,
    /// Last message receive time
    last_recv_time: SystemTime,
}

impl ClientSession {
    /// Create a new client session
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            state: SessionState::Disconnected,
            msg_seq_num_out: Arc::new(AtomicU64::new(1)),
            msg_seq_num_in: Arc::new(AtomicU64::new(1)),
            parser: Parser::new(),
            heartbeat_timer: None,
            last_send_time: SystemTime::now(),
            last_recv_time: SystemTime::now(),
        }
    }

    /// Get the sender CompID
    pub fn sender_comp_id(&self) -> &str {
        &self.config.sender_comp_id
    }

    /// Get the target CompID
    pub fn target_comp_id(&self) -> &str {
        &self.config.target_comp_id
    }

    /// Get current session state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Get next outgoing sequence number (and increment)
    fn next_seq_num_out(&self) -> u64 {
        self.msg_seq_num_out.fetch_add(1, Ordering::SeqCst)
    }

    /// Get current outgoing sequence number (without incrementing)
    pub fn current_seq_num_out(&self) -> u64 {
        self.msg_seq_num_out.load(Ordering::SeqCst)
    }

    /// Get expected incoming sequence number
    pub fn expected_seq_num_in(&self) -> u64 {
        self.msg_seq_num_in.load(Ordering::SeqCst)
    }

    /// Reset sequence numbers (called on new session)
    pub fn reset_sequence_numbers(&self) {
        self.msg_seq_num_out.store(1, Ordering::SeqCst);
        self.msg_seq_num_in.store(1, Ordering::SeqCst);
    }

    /// Generate FIX timestamp (UTC, format: YYYYMMDD-HH:MM:SS.sss)
    pub fn fix_timestamp() -> String {
        let now = Utc::now();
        now.format("%Y%m%d-%H:%M:%S%.3f").to_string()
    }

    /// Create Logon message with HMAC authentication
    pub fn create_logon_message(&self) -> Vec<u8> {
        let seq_num = self.next_seq_num_out();
        let sending_time = Self::fix_timestamp();

        // Compute HMAC signature
        let signature = compute_logon_hmac(
            &self.config.api_secret,
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            "A", // Logon
            seq_num,
            &sending_time,
            &self.config.api_passphrase,
        );

        // Build Logon message
        let mut msg = FixMessage::new(
            MsgType::Logon,
            seq_num,
            self.config.sender_comp_id.clone(),
            self.config.target_comp_id.clone(),
            sending_time,
        );

        msg.add_field(Tag::EncryptMethod, "0".to_string()); // None
        msg.add_field(Tag::HeartBtInt, self.config.heartbeat_interval.to_string());
        msg.add_field(Tag::Username, self.config.sender_comp_id.clone());
        msg.add_field(Tag::Password, self.config.api_passphrase.clone()); // Tag 554
        msg.add_field(Tag::RawDataLength, signature.len().to_string()); // Tag 95
        msg.add_field(Tag::RawData, signature); // Tag 96
        msg.add_field(Tag::ResetSeqNumFlag, "Y".to_string()); // Reset sequence numbers
        msg.add_field(Tag::DefaultApplVerID, "9".to_string()); // Tag 1137 = FIX 5.0 SP2
        msg.add_field(Tag::CancelOrdersOnDisconnect, "Y".to_string()); // Tag 8013

        build_fix_message(&msg)
    }

    /// Create Heartbeat message
    pub fn create_heartbeat_message(&self, test_req_id: Option<&str>) -> Vec<u8> {
        let seq_num = self.next_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::Heartbeat,
            seq_num,
            self.config.sender_comp_id.clone(),
            self.config.target_comp_id.clone(),
            sending_time,
        );

        // If responding to TestRequest, include TestReqID
        if let Some(id) = test_req_id {
            msg.add_field(Tag::TestReqID, id.to_string());
        }

        build_fix_message(&msg)
    }

    /// Create TestRequest message
    pub fn create_test_request_message(&self, test_req_id: &str) -> Vec<u8> {
        let seq_num = self.next_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::TestRequest,
            seq_num,
            self.config.sender_comp_id.clone(),
            self.config.target_comp_id.clone(),
            sending_time,
        );

        msg.add_field(Tag::TestReqID, test_req_id.to_string());

        build_fix_message(&msg)
    }

    /// Create Logout message
    pub fn create_logout_message(&self) -> Vec<u8> {
        let seq_num = self.next_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let msg = FixMessage::new(
            MsgType::Logout,
            seq_num,
            self.config.sender_comp_id.clone(),
            self.config.target_comp_id.clone(),
            sending_time,
        );

        build_fix_message(&msg)
    }

    /// Create ResendRequest message for gap recovery
    pub fn create_resend_request(&self, begin_seq_no: u64, end_seq_no: u64) -> Vec<u8> {
        let seq_num = self.next_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::ResendRequest,
            seq_num,
            self.config.sender_comp_id.clone(),
            self.config.target_comp_id.clone(),
            sending_time,
        );

        msg.add_field(Tag::BeginSeqNo, begin_seq_no.to_string());
        msg.add_field(Tag::EndSeqNo, end_seq_no.to_string());

        build_fix_message(&msg)
    }

    /// Process received message data
    pub fn feed_data(&mut self, data: &[u8]) {
        self.parser.feed(data);
        self.last_recv_time = SystemTime::now();
    }

    /// Try to parse next message from buffer
    pub fn parse_next_message(&mut self) -> Result<Option<FixMessage>, ParseError> {
        self.parser.parse_message()
    }

    /// Check if sequence number is expected (for gap detection)
    pub fn check_sequence_number(&self, received_seq_num: u64) -> bool {
        let expected = self.msg_seq_num_in.load(Ordering::SeqCst);
        received_seq_num == expected
    }

    /// Increment expected incoming sequence number
    pub fn increment_seq_num_in(&self) {
        self.msg_seq_num_in.fetch_add(1, Ordering::SeqCst);
    }

    /// Set session state
    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;

        // Setup heartbeat timer when logged in
        if state == SessionState::LoggedIn && self.heartbeat_timer.is_none() {
            let interval_duration =
                Duration::from_secs(((self.config.heartbeat_interval as f64) * 0.75) as u64);
            self.heartbeat_timer = Some(interval(interval_duration));
        }
    }

    /// Check if heartbeat should be sent (idle time > HeartBtInt × 0.75)
    pub fn should_send_heartbeat(&self) -> bool {
        if self.state != SessionState::LoggedIn {
            return false;
        }

        let idle_time = self.last_send_time.elapsed().unwrap_or(Duration::ZERO);
        let threshold =
            Duration::from_secs(((self.config.heartbeat_interval as f64) * 0.75) as u64);

        idle_time >= threshold
    }

    /// Update last send time
    pub fn update_last_send_time(&mut self) {
        self.last_send_time = SystemTime::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SessionConfig {
        SessionConfig {
            sender_comp_id: "TEST_API_KEY".to_string(),
            target_comp_id: "Coinbase".to_string(),
            api_secret: "test_secret".to_string(),
            api_passphrase: "test_passphrase".to_string(),
            heartbeat_interval: 30,
        }
    }

    #[test]
    fn test_session_creation() {
        let session = ClientSession::new(test_config());
        assert_eq!(session.state(), SessionState::Disconnected);
        assert_eq!(session.current_seq_num_out(), 1);
        assert_eq!(session.expected_seq_num_in(), 1);
    }

    #[test]
    fn test_sequence_numbers() {
        let session = ClientSession::new(test_config());

        assert_eq!(session.next_seq_num_out(), 1);
        assert_eq!(session.next_seq_num_out(), 2);
        assert_eq!(session.next_seq_num_out(), 3);

        session.reset_sequence_numbers();
        assert_eq!(session.current_seq_num_out(), 1);
    }

    #[test]
    fn test_logon_message_creation() {
        let session = ClientSession::new(test_config());
        let logon = session.create_logon_message();

        // Parse it back to verify structure
        let mut parser = Parser::new();
        parser.feed(&logon);
        let parsed = parser.parse_message().unwrap().unwrap();

        assert_eq!(parsed.msg_type, MsgType::Logon);
        assert_eq!(parsed.sender_comp_id, "TEST_API_KEY");
        assert_eq!(parsed.target_comp_id, "Coinbase");
        assert_eq!(parsed.get_field(Tag::HeartBtInt), Some("30"));
        assert!(parsed.get_field(Tag::RawData).is_some()); // HMAC signature
    }

    #[test]
    fn test_heartbeat_message_creation() {
        let session = ClientSession::new(test_config());
        let heartbeat = session.create_heartbeat_message(None);

        let mut parser = Parser::new();
        parser.feed(&heartbeat);
        let parsed = parser.parse_message().unwrap().unwrap();

        assert_eq!(parsed.msg_type, MsgType::Heartbeat);
    }

    #[test]
    fn test_heartbeat_with_test_req_id() {
        let session = ClientSession::new(test_config());
        let heartbeat = session.create_heartbeat_message(Some("TEST123"));

        let mut parser = Parser::new();
        parser.feed(&heartbeat);
        let parsed = parser.parse_message().unwrap().unwrap();

        assert_eq!(parsed.msg_type, MsgType::Heartbeat);
        assert_eq!(parsed.get_field(Tag::TestReqID), Some("TEST123"));
    }

    #[test]
    fn test_sequence_gap_detection() {
        let session = ClientSession::new(test_config());

        assert!(session.check_sequence_number(1)); // Expected
        assert!(!session.check_sequence_number(2)); // Gap!

        session.increment_seq_num_in();
        assert!(session.check_sequence_number(2)); // Now expected
    }
}
