// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Market data message types and builders.
//!
//! This module provides structures and functions for working with FIX market data messages,
//! including MarketDataRequest, MarketDataIncrementalRefresh, SecurityList, and SecurityStatus.
//!
//! Key features:
//! - Zero-copy parsing where possible
//! - Repeating group handling (NoMDEntries, NoRelatedSym)
//! - Coinbase-specific L3 order book updates
//! - Trading status monitoring

use crate::message::{FixMessage, MsgType, Tag};
use crate::serialization::build_fix_message;
use rust_decimal::Decimal;
use std::str::FromStr;

/// Subscription request type for market data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionRequestType {
    /// Snapshot (0)
    Snapshot,
    /// Subscribe (1)
    Subscribe,
    /// Unsubscribe (2)
    Unsubscribe,
}

impl SubscriptionRequestType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubscriptionRequestType::Snapshot => "0",
            SubscriptionRequestType::Subscribe => "1",
            SubscriptionRequestType::Unsubscribe => "2",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(SubscriptionRequestType::Snapshot),
            "1" => Some(SubscriptionRequestType::Subscribe),
            "2" => Some(SubscriptionRequestType::Unsubscribe),
            _ => None,
        }
    }
}

/// Market data update type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MDUpdateType {
    /// Full refresh (0)
    FullRefresh,
    /// Incremental refresh (1)
    IncrementalRefresh,
}

impl MDUpdateType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MDUpdateType::FullRefresh => "0",
            MDUpdateType::IncrementalRefresh => "1",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(MDUpdateType::FullRefresh),
            "1" => Some(MDUpdateType::IncrementalRefresh),
            _ => None,
        }
    }
}

/// Market data entry type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MDEntryType {
    /// Bid (0)
    Bid,
    /// Offer (1)
    Offer,
    /// Trade (2)
    Trade,
}

impl MDEntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MDEntryType::Bid => "0",
            MDEntryType::Offer => "1",
            MDEntryType::Trade => "2",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(MDEntryType::Bid),
            "1" => Some(MDEntryType::Offer),
            "2" => Some(MDEntryType::Trade),
            _ => None,
        }
    }
}

/// Market data update action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MDUpdateAction {
    /// New (0)
    New,
    /// Change (1)
    Change,
    /// Delete (2)
    Delete,
}

impl MDUpdateAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            MDUpdateAction::New => "0",
            MDUpdateAction::Change => "1",
            MDUpdateAction::Delete => "2",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(MDUpdateAction::New),
            "1" => Some(MDUpdateAction::Change),
            "2" => Some(MDUpdateAction::Delete),
            _ => None,
        }
    }
}

/// Builder for MarketDataRequest messages
pub struct MarketDataRequestBuilder {
    msg: FixMessage,
}

impl MarketDataRequestBuilder {
    /// Create a new MarketDataRequest builder
    pub fn new(
        seq_num: u64,
        sender_comp_id: String,
        target_comp_id: String,
        sending_time: String,
        md_req_id: String,
    ) -> Self {
        let mut msg = FixMessage::new(
            MsgType::MarketDataRequest,
            seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
        );

        msg.add_field(Tag::MDReqID, md_req_id);
        // Default to 1 symbol in NoRelatedSym (will be updated if multiple symbols added)
        msg.add_field(Tag::NoRelatedSym, "1".to_string());

        Self { msg }
    }

    /// Set subscription request type
    pub fn subscription_request_type(mut self, req_type: SubscriptionRequestType) -> Self {
        self.msg
            .add_field(Tag::SubscriptionRequestType, req_type.as_str().to_string());
        self
    }

    /// Set market depth (0 = full book)
    pub fn market_depth(mut self, depth: u32) -> Self {
        self.msg.add_field(Tag::MarketDepth, depth.to_string());
        self
    }

    /// Set market data update type
    pub fn md_update_type(mut self, update_type: MDUpdateType) -> Self {
        self.msg
            .add_field(Tag::MDUpdateType, update_type.as_str().to_string());
        self
    }

    /// Add a symbol to subscribe to
    pub fn add_symbol(mut self, symbol: String) -> Self {
        self.msg.add_field(Tag::Symbol, symbol);
        self
    }

    /// Build the FIX message bytes
    pub fn build(self) -> Vec<u8> {
        build_fix_message(&self.msg)
    }
}

/// Builder for SecurityListRequest messages
pub struct SecurityListRequestBuilder {
    msg: FixMessage,
}

impl SecurityListRequestBuilder {
    /// Create a new SecurityListRequest builder
    pub fn new(
        seq_num: u64,
        sender_comp_id: String,
        target_comp_id: String,
        sending_time: String,
        security_req_id: String,
    ) -> Self {
        let mut msg = FixMessage::new(
            MsgType::SecurityListRequest,
            seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
        );

        msg.add_field(Tag::SecurityReqID, security_req_id);

        Self { msg }
    }

    /// Build the FIX message bytes
    pub fn build(self) -> Vec<u8> {
        build_fix_message(&self.msg)
    }
}

/// Represents a single market data entry in an incremental refresh
#[derive(Debug, Clone, PartialEq)]
pub struct MDEntry {
    /// Update action (New/Change/Delete)
    pub update_action: MDUpdateAction,
    /// Entry type (Bid/Offer/Trade)
    pub entry_type: MDEntryType,
    /// Price level (exact decimal arithmetic)
    pub price: Decimal,
    /// Size at this level (exact decimal arithmetic)
    pub size: Decimal,
    /// Unique entry ID (for tracking)
    pub entry_id: Option<String>,
}

/// Parsed MarketDataIncrementalRefresh message
#[derive(Debug, Clone, PartialEq)]
pub struct MarketDataIncrementalRefresh {
    /// MD Request ID
    pub md_req_id: Option<String>,
    /// Market data entries
    pub entries: Vec<MDEntry>,
}

impl MarketDataIncrementalRefresh {
    /// Parse a MarketDataIncrementalRefresh from a FixMessage
    ///
    /// Parses the NoMDEntries repeating group to extract individual market data entries.
    /// Each entry contains update action, entry type, price, size, and optional entry ID.
    /// Uses rust_decimal for exact decimal arithmetic on prices and sizes.
    pub fn from_fix_message(msg: &FixMessage) -> Option<Self> {
        if msg.msg_type != MsgType::MarketDataIncrementalRefresh {
            return None;
        }

        let md_req_id = msg.get_field(Tag::MDReqID).map(|s| s.to_string());

        // Get the number of entries from NoMDEntries (tag 268)
        let num_entries: usize = msg.get_field(Tag::NoMDEntries)?.parse().ok()?;

        // Parse repeating group entries
        // In FIX, repeating groups are sequential in the fields array
        // We need to find where the group starts and parse each entry
        let mut entries = Vec::with_capacity(num_entries);
        let mut current_entry: Option<(
            Option<MDUpdateAction>,
            Option<MDEntryType>,
            Option<Decimal>,
            Option<Decimal>,
            Option<String>,
        )> = None;

        for (tag, value) in &msg.fields {
            match tag {
                Tag::MDUpdateAction => {
                    // Start of a new entry - save previous if exists
                    if let Some((
                        Some(action),
                        Some(entry_type),
                        Some(price),
                        Some(size),
                        entry_id,
                    )) = current_entry.take()
                    {
                        entries.push(MDEntry {
                            update_action: action,
                            entry_type,
                            price,
                            size,
                            entry_id,
                        });
                    }

                    let action = MDUpdateAction::from_str(value)?;
                    current_entry = Some((Some(action), None, None, None, None));
                }
                Tag::MDEntryType => {
                    if let Some(ref mut entry) = current_entry {
                        entry.1 = MDEntryType::from_str(value);
                    }
                }
                Tag::MDEntryPx => {
                    if let Some(ref mut entry) = current_entry {
                        entry.2 = Decimal::from_str(value).ok();
                    }
                }
                Tag::MDEntrySize => {
                    if let Some(ref mut entry) = current_entry {
                        entry.3 = Decimal::from_str(value).ok();
                    }
                }
                Tag::MDEntryID => {
                    if let Some(ref mut entry) = current_entry {
                        entry.4 = Some(value.to_string());
                    }
                }
                _ => {
                    // Ignore other fields for now
                }
            }
        }

        // Don't forget the last entry
        if let Some((Some(action), Some(entry_type), Some(price), Some(size), entry_id)) =
            current_entry
        {
            entries.push(MDEntry {
                update_action: action,
                entry_type,
                price,
                size,
                entry_id,
            });
        }

        Some(Self { md_req_id, entries })
    }
}

/// Parsed SecurityList message
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityList {
    /// Security request ID
    pub security_req_id: String,
    /// Security response ID
    pub security_response_id: String,
    /// List of symbols
    pub symbols: Vec<String>,
}

impl SecurityList {
    /// Parse a SecurityList from a FixMessage
    ///
    /// Parses the NoRelatedSym repeating group to extract available trading symbols.
    pub fn from_fix_message(msg: &FixMessage) -> Option<Self> {
        if msg.msg_type != MsgType::SecurityList {
            return None;
        }

        let security_req_id = msg.get_field(Tag::SecurityReqID)?.to_string();
        let security_response_id = msg.get_field(Tag::SecurityResponseID)?.to_string();

        // Get the number of symbols from NoRelatedSym (tag 146)
        let num_symbols: usize = msg.get_field(Tag::NoRelatedSym)?.parse().ok()?;

        // Parse repeating group - each symbol is marked by tag 55 (Symbol)
        let mut symbols = Vec::with_capacity(num_symbols);

        for (tag, value) in &msg.fields {
            if *tag == Tag::Symbol {
                symbols.push(value.to_string());
            }
        }

        Some(Self {
            security_req_id,
            security_response_id,
            symbols,
        })
    }
}

/// Trading status from SecurityStatus message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingStatus {
    /// Trading halt (2)
    TradingHalt,
    /// Resume (3)
    Resume,
    /// No open / No resume (4)
    NoOpenNoResume,
}

impl TradingStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "2" => Some(TradingStatus::TradingHalt),
            "3" => Some(TradingStatus::Resume),
            "4" => Some(TradingStatus::NoOpenNoResume),
            _ => None,
        }
    }
}

/// Parsed SecurityStatus message
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityStatus {
    /// Symbol
    pub symbol: String,
    /// Trading status
    pub trading_status: TradingStatus,
}

impl SecurityStatus {
    /// Parse a SecurityStatus from a FixMessage
    pub fn from_fix_message(msg: &FixMessage) -> Option<Self> {
        if msg.msg_type != MsgType::SecurityStatus {
            return None;
        }

        let symbol = msg.get_field(Tag::Symbol)?.to_string();
        let status_str = msg.get_field(Tag::SecurityTradingStatus)?;
        let trading_status = TradingStatus::from_str(status_str)?;

        Some(Self {
            symbol,
            trading_status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_request_type_conversion() {
        assert_eq!(SubscriptionRequestType::Subscribe.as_str(), "1");
        assert_eq!(
            SubscriptionRequestType::from_str("1"),
            Some(SubscriptionRequestType::Subscribe)
        );
    }

    #[test]
    fn test_md_update_type_conversion() {
        assert_eq!(MDUpdateType::IncrementalRefresh.as_str(), "1");
        assert_eq!(
            MDUpdateType::from_str("1"),
            Some(MDUpdateType::IncrementalRefresh)
        );
    }

    #[test]
    fn test_md_entry_type_conversion() {
        assert_eq!(MDEntryType::Bid.as_str(), "0");
        assert_eq!(MDEntryType::Offer.as_str(), "1");
        assert_eq!(MDEntryType::from_str("0"), Some(MDEntryType::Bid));
    }

    #[test]
    fn test_md_update_action_conversion() {
        assert_eq!(MDUpdateAction::New.as_str(), "0");
        assert_eq!(MDUpdateAction::Change.as_str(), "1");
        assert_eq!(MDUpdateAction::Delete.as_str(), "2");
    }

    #[test]
    fn test_market_data_request_builder() {
        let request = MarketDataRequestBuilder::new(
            1,
            "CLIENT".to_string(),
            "COINBASE".to_string(),
            "20260212-12:00:00.000".to_string(),
            "REQ123".to_string(),
        )
        .subscription_request_type(SubscriptionRequestType::Subscribe)
        .market_depth(0)
        .md_update_type(MDUpdateType::IncrementalRefresh)
        .add_symbol("BTC-USD".to_string())
        .build();

        // Verify it's a valid FIX message by parsing it
        use crate::parser::Parser;
        let mut parser = Parser::new();
        parser.feed(&request);
        let parsed = parser.parse_message().unwrap().unwrap();

        assert_eq!(parsed.msg_type, MsgType::MarketDataRequest);
        assert_eq!(parsed.get_field(Tag::MDReqID), Some("REQ123"));
        assert_eq!(parsed.get_field(Tag::SubscriptionRequestType), Some("1"));
        assert_eq!(parsed.get_field(Tag::MarketDepth), Some("0"));
        assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC-USD"));
    }

    #[test]
    fn test_security_list_request_builder() {
        let request = SecurityListRequestBuilder::new(
            1,
            "CLIENT".to_string(),
            "COINBASE".to_string(),
            "20260212-12:00:00.000".to_string(),
            "SECREQ123".to_string(),
        )
        .build();

        // Verify it's a valid FIX message
        use crate::parser::Parser;
        let mut parser = Parser::new();
        parser.feed(&request);
        let parsed = parser.parse_message().unwrap().unwrap();

        assert_eq!(parsed.msg_type, MsgType::SecurityListRequest);
        assert_eq!(parsed.get_field(Tag::SecurityReqID), Some("SECREQ123"));
    }

    #[test]
    fn test_security_status_parsing() {
        let mut msg = FixMessage::new(
            MsgType::SecurityStatus,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::SecurityTradingStatus, "2".to_string()); // Trading halt

        let status = SecurityStatus::from_fix_message(&msg).unwrap();
        assert_eq!(status.symbol, "BTC-USD");
        assert_eq!(status.trading_status, TradingStatus::TradingHalt);
    }

    #[test]
    fn test_trading_status_conversion() {
        assert_eq!(
            TradingStatus::from_str("2"),
            Some(TradingStatus::TradingHalt)
        );
        assert_eq!(TradingStatus::from_str("3"), Some(TradingStatus::Resume));
    }

    #[test]
    fn test_market_data_incremental_refresh_parsing_single_entry() {
        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::MDReqID, "REQ123".to_string());
        msg.add_field(Tag::NoMDEntries, "1".to_string());

        // Single entry
        msg.add_field(Tag::MDUpdateAction, "0".to_string()); // New
        msg.add_field(Tag::MDEntryType, "0".to_string()); // Bid
        msg.add_field(Tag::MDEntryPx, "50000.00".to_string());
        msg.add_field(Tag::MDEntrySize, "1.5".to_string());
        msg.add_field(Tag::MDEntryID, "ORDER123".to_string());

        let refresh = MarketDataIncrementalRefresh::from_fix_message(&msg).unwrap();
        assert_eq!(refresh.md_req_id, Some("REQ123".to_string()));
        assert_eq!(refresh.entries.len(), 1);

        let entry = &refresh.entries[0];
        assert_eq!(entry.update_action, MDUpdateAction::New);
        assert_eq!(entry.entry_type, MDEntryType::Bid);
        assert_eq!(entry.price, Decimal::from_str("50000.00").unwrap());
        assert_eq!(entry.size, Decimal::from_str("1.5").unwrap());
        assert_eq!(entry.entry_id, Some("ORDER123".to_string()));
    }

    #[test]
    fn test_market_data_incremental_refresh_parsing_multiple_entries() {
        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::NoMDEntries, "3".to_string());

        // First entry - New bid
        msg.add_field(Tag::MDUpdateAction, "0".to_string());
        msg.add_field(Tag::MDEntryType, "0".to_string());
        msg.add_field(Tag::MDEntryPx, "50000.00".to_string());
        msg.add_field(Tag::MDEntrySize, "1.5".to_string());
        msg.add_field(Tag::MDEntryID, "ORDER1".to_string());

        // Second entry - Change offer
        msg.add_field(Tag::MDUpdateAction, "1".to_string());
        msg.add_field(Tag::MDEntryType, "1".to_string());
        msg.add_field(Tag::MDEntryPx, "50001.00".to_string());
        msg.add_field(Tag::MDEntrySize, "2.0".to_string());
        msg.add_field(Tag::MDEntryID, "ORDER2".to_string());

        // Third entry - Delete bid
        msg.add_field(Tag::MDUpdateAction, "2".to_string());
        msg.add_field(Tag::MDEntryType, "0".to_string());
        msg.add_field(Tag::MDEntryPx, "49999.00".to_string());
        msg.add_field(Tag::MDEntrySize, "0.0".to_string());
        msg.add_field(Tag::MDEntryID, "ORDER3".to_string());

        let refresh = MarketDataIncrementalRefresh::from_fix_message(&msg).unwrap();
        assert_eq!(refresh.entries.len(), 3);

        // Verify first entry
        assert_eq!(refresh.entries[0].update_action, MDUpdateAction::New);
        assert_eq!(refresh.entries[0].entry_type, MDEntryType::Bid);
        assert_eq!(
            refresh.entries[0].price,
            Decimal::from_str("50000.00").unwrap()
        );

        // Verify second entry
        assert_eq!(refresh.entries[1].update_action, MDUpdateAction::Change);
        assert_eq!(refresh.entries[1].entry_type, MDEntryType::Offer);
        assert_eq!(
            refresh.entries[1].price,
            Decimal::from_str("50001.00").unwrap()
        );

        // Verify third entry
        assert_eq!(refresh.entries[2].update_action, MDUpdateAction::Delete);
        assert_eq!(refresh.entries[2].entry_type, MDEntryType::Bid);
        assert_eq!(refresh.entries[2].entry_id, Some("ORDER3".to_string()));
    }

    #[test]
    fn test_market_data_incremental_refresh_wrong_message_type() {
        let msg = FixMessage::new(
            MsgType::Heartbeat,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let result = MarketDataIncrementalRefresh::from_fix_message(&msg);
        assert!(result.is_none());
    }

    #[test]
    fn test_security_list_parsing() {
        let mut msg = FixMessage::new(
            MsgType::SecurityList,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::SecurityReqID, "SECREQ1".to_string());
        msg.add_field(Tag::SecurityResponseID, "SECRESP1".to_string());
        msg.add_field(Tag::NoRelatedSym, "3".to_string());

        // Add symbols
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Symbol, "ETH-USD".to_string());
        msg.add_field(Tag::Symbol, "SOL-USD".to_string());

        let list = SecurityList::from_fix_message(&msg).unwrap();
        assert_eq!(list.security_req_id, "SECREQ1");
        assert_eq!(list.security_response_id, "SECRESP1");
        assert_eq!(list.symbols.len(), 3);
        assert_eq!(list.symbols[0], "BTC-USD");
        assert_eq!(list.symbols[1], "ETH-USD");
        assert_eq!(list.symbols[2], "SOL-USD");
    }

    #[test]
    fn test_security_list_single_symbol() {
        let mut msg = FixMessage::new(
            MsgType::SecurityList,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::SecurityReqID, "REQ1".to_string());
        msg.add_field(Tag::SecurityResponseID, "RESP1".to_string());
        msg.add_field(Tag::NoRelatedSym, "1".to_string());
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());

        let list = SecurityList::from_fix_message(&msg).unwrap();
        assert_eq!(list.symbols.len(), 1);
        assert_eq!(list.symbols[0], "BTC-USD");
    }

    #[test]
    fn test_market_data_incremental_refresh_empty_entries() {
        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::NoMDEntries, "0".to_string());

        let refresh = MarketDataIncrementalRefresh::from_fix_message(&msg).unwrap();
        assert_eq!(refresh.entries.len(), 0);
    }

    #[test]
    fn test_market_data_incremental_refresh_missing_no_md_entries() {
        let msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let result = MarketDataIncrementalRefresh::from_fix_message(&msg);
        assert!(result.is_none());
    }

    #[test]
    fn test_security_list_missing_fields() {
        let msg = FixMessage::new(
            MsgType::SecurityList,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let result = SecurityList::from_fix_message(&msg);
        assert!(result.is_none());
    }

    #[test]
    fn test_security_status_missing_symbol() {
        let mut msg = FixMessage::new(
            MsgType::SecurityStatus,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::SecurityTradingStatus, "2".to_string());

        let result = SecurityStatus::from_fix_message(&msg);
        assert!(result.is_none());
    }

    #[test]
    fn test_market_data_incremental_refresh_max_entries() {
        let mut msg = FixMessage::new(
            MsgType::MarketDataIncrementalRefresh,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        // Stress test with 100 entries
        msg.add_field(Tag::NoMDEntries, "100".to_string());

        for i in 0..100 {
            let action = match i % 3 {
                0 => "0", // New
                1 => "1", // Change
                _ => "2", // Delete
            };
            let entry_type = if i % 2 == 0 { "0" } else { "1" };
            let price = format!("{}.{:02}", 50000 + i, i % 100);
            let size = format!("{}.{}", 1 + (i % 10), i % 10);
            let entry_id = format!("ORDER{}", i);

            msg.add_field(Tag::MDUpdateAction, action.to_string());
            msg.add_field(Tag::MDEntryType, entry_type.to_string());
            msg.add_field(Tag::MDEntryPx, price);
            msg.add_field(Tag::MDEntrySize, size);
            msg.add_field(Tag::MDEntryID, entry_id);
        }

        let refresh = MarketDataIncrementalRefresh::from_fix_message(&msg).unwrap();
        assert_eq!(refresh.entries.len(), 100);

        // Verify first and last entries
        assert_eq!(refresh.entries[0].update_action, MDUpdateAction::New);
        assert_eq!(
            refresh.entries[0].price,
            Decimal::from_str("50000.00").unwrap()
        );

        // Entry 99: 99 % 3 = 0, so action is New
        assert_eq!(refresh.entries[99].update_action, MDUpdateAction::New);
        assert_eq!(
            refresh.entries[99].price,
            Decimal::from_str("50099.99").unwrap()
        );
    }
}
