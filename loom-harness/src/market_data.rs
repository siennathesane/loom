// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Market data simulator for test harness.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::serialization::build_fix_message;
use loom_fix::session::server::ServerSession;
use std::time::{SystemTime, UNIX_EPOCH};

/// Market data update entry
#[derive(Debug, Clone)]
pub struct MarketDataUpdate {
    pub instrument: String,
    pub price: f64,
    pub size: f64,
    pub timestamp: u64,
}

/// Market data simulator
pub struct MarketDataSimulator {
    instruments: Vec<String>,
    updates: Vec<MarketDataUpdate>,
}

impl MarketDataSimulator {
    /// Create a new market data simulator
    pub fn new() -> Self {
        Self {
            instruments: vec![
                "BTC-USD".to_string(),
                "ETH-USD".to_string(),
                "SOL-USD".to_string(),
            ],
            updates: Vec::new(),
        }
    }

    /// Set available instruments
    pub fn set_instruments(&mut self, instruments: Vec<String>) {
        self.instruments = instruments;
    }

    /// Add a market data update
    pub fn add_update(&mut self, instrument: String, price: f64, size: f64) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        self.updates.push(MarketDataUpdate {
            instrument,
            price,
            size,
            timestamp,
        });
    }

    /// Create SecurityList response
    pub fn create_security_list_response(&self, session: &ServerSession) -> Vec<u8> {
        let seq_num = session.current_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::SecurityList,
            seq_num,
            "Coinbase".to_string(),
            "CLIENT".to_string(), // Will be overridden by server
            sending_time,
        );

        // SecurityReqID from request (for now, use a fixed value)
        msg.add_field(Tag::SecurityReqID, "SEC_LIST_REQ_1".to_string());
        msg.add_field(Tag::SecurityResponseID, "SEC_LIST_RESP_1".to_string());

        // Number of instruments
        msg.add_field(Tag::NoRelatedSym, self.instruments.len().to_string());

        // For each instrument, we'd add repeating group fields
        // For simplicity in this test harness, we'll just indicate the count
        // Real implementation would use repeating groups per FIX spec

        build_fix_message(&msg)
    }

    /// Create MarketDataIncrementalRefresh message
    pub fn create_incremental_refresh(&self, session: &ServerSession, count: usize) -> Vec<u8> {
        let seq_num = session.current_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            seq_num,
            "Coinbase".to_string(),
            "CLIENT".to_string(),
            sending_time,
        );

        // Number of MD entries
        msg.add_field(Tag::NoMDEntries, count.to_string());

        // In a real implementation, we'd add repeating group fields here
        // For testing, the count is sufficient

        build_fix_message(&msg)
    }

    /// Create SecurityStatus message
    pub fn create_security_status(
        &self,
        session: &ServerSession,
        instrument: &str,
        status: &str,
    ) -> Vec<u8> {
        let seq_num = session.current_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::SecurityStatus,
            seq_num,
            "Coinbase".to_string(),
            "CLIENT".to_string(),
            sending_time,
        );

        msg.add_field(Tag::Symbol, instrument.to_string());
        msg.add_field(Tag::SecurityTradingStatus, status.to_string());

        build_fix_message(&msg)
    }

    /// Generate FIX timestamp
    fn fix_timestamp() -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = now.as_secs();
        let millis = now.subsec_millis();

        let days_since_epoch = secs / 86400;
        let secs_today = secs % 86400;

        let year = 1970 + (days_since_epoch / 365);
        let month = ((days_since_epoch % 365) / 30) + 1;
        let day = ((days_since_epoch % 365) % 30) + 1;

        let hour = secs_today / 3600;
        let minute = (secs_today % 3600) / 60;
        let second = secs_today % 60;

        format!(
            "{:04}{:02}{:02}-{:02}:{:02}:{:02}.{:03}",
            year, month, day, hour, minute, second, millis
        )
    }
}

impl Default for MarketDataSimulator {
    fn default() -> Self {
        Self::new()
    }
}
