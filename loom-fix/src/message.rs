// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! FIX message types and tag definitions.
//!
//! This module defines the FIX tags and message types used by Coinbase's
//! FIX 5.0 SP2 implementation.

use std::fmt;

/// FIX message type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    /// Heartbeat (0)
    Heartbeat,
    /// TestRequest (1)
    TestRequest,
    /// ResendRequest (2)
    ResendRequest,
    /// Reject (3)
    Reject,
    /// SequenceReset (4)
    SequenceReset,
    /// Logout (5)
    Logout,
    /// Logon (A)
    Logon,
    /// NewOrderSingle (D)
    NewOrderSingle,
    /// ExecutionReport (8)
    ExecutionReport,
    /// OrderCancelRequest (F)
    OrderCancelRequest,
    /// OrderCancelReject (9)
    OrderCancelReject,
    /// MarketDataRequest (V)
    MarketDataRequest,
    /// MarketDataIncrementalRefresh (X)
    MarketDataIncrementalRefresh,
    /// MarketDataRequestReject (Y)
    MarketDataRequestReject,
    /// SecurityListRequest (x)
    SecurityListRequest,
    /// SecurityList (y)
    SecurityList,
    /// SecurityStatus (f)
    SecurityStatus,
    /// MarketDataSnapshotFullRefresh (W)
    MarketDataSnapshotFullRefresh,
    /// NewOrderBatch (U6)
    NewOrderBatch,
}

impl MsgType {
    /// Convert MsgType to its FIX string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            MsgType::Heartbeat => "0",
            MsgType::TestRequest => "1",
            MsgType::ResendRequest => "2",
            MsgType::Reject => "3",
            MsgType::SequenceReset => "4",
            MsgType::Logout => "5",
            MsgType::Logon => "A",
            MsgType::NewOrderSingle => "D",
            MsgType::ExecutionReport => "8",
            MsgType::OrderCancelRequest => "F",
            MsgType::OrderCancelReject => "9",
            MsgType::MarketDataRequest => "V",
            MsgType::MarketDataIncrementalRefresh => "X",
            MsgType::MarketDataRequestReject => "Y",
            MsgType::SecurityListRequest => "x",
            MsgType::SecurityList => "y",
            MsgType::SecurityStatus => "f",
            MsgType::MarketDataSnapshotFullRefresh => "W",
            MsgType::NewOrderBatch => "U6",
        }
    }

    /// Parse MsgType from FIX string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(MsgType::Heartbeat),
            "1" => Some(MsgType::TestRequest),
            "2" => Some(MsgType::ResendRequest),
            "3" => Some(MsgType::Reject),
            "4" => Some(MsgType::SequenceReset),
            "5" => Some(MsgType::Logout),
            "A" => Some(MsgType::Logon),
            "D" => Some(MsgType::NewOrderSingle),
            "8" => Some(MsgType::ExecutionReport),
            "F" => Some(MsgType::OrderCancelRequest),
            "9" => Some(MsgType::OrderCancelReject),
            "V" => Some(MsgType::MarketDataRequest),
            "X" => Some(MsgType::MarketDataIncrementalRefresh),
            "Y" => Some(MsgType::MarketDataRequestReject),
            "x" => Some(MsgType::SecurityListRequest),
            "y" => Some(MsgType::SecurityList),
            "f" => Some(MsgType::SecurityStatus),
            "W" => Some(MsgType::MarketDataSnapshotFullRefresh),
            "U6" => Some(MsgType::NewOrderBatch),
            _ => None,
        }
    }
}

impl fmt::Display for MsgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// FIX tag numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Tag {
    // Standard header tags
    BeginString = 8,
    BodyLength = 9,
    MsgType = 35,
    MsgSeqNum = 34,
    SenderCompID = 49,
    SendingTime = 52,
    TargetCompID = 56,

    // Session-level tags
    TestReqID = 112,
    HeartBtInt = 108,
    EncryptMethod = 98,
    Password = 554, // Tag 554 = Password (passphrase)
    Username = 553,
    RawDataLength = 95, // Tag 95 = RawDataLength
    RawData = 96,       // Tag 96 = RawData (HMAC signature)
    ResetSeqNumFlag = 141,

    // Resend request tags
    BeginSeqNo = 7,
    EndSeqNo = 16,
    NewSeqNo = 36,

    // Order entry tags
    ClOrdID = 11,
    Symbol = 55,
    Side = 54,
    OrderQty = 38,
    OrdType = 40,
    Price = 44,
    TimeInForce = 59,
    TransactTime = 60,

    // Additional order entry tags
    OrigClOrdID = 41,
    CashOrderQty = 152,
    StopPx = 99,
    ExecInst = 18,
    ExpireTime = 126,
    DropCopyFlag = 9406,
    DefaultApplVerID = 1137,
    NoOrders = 73,
    CxlRejReason = 102,
    CxlRejResponseTo = 434,
    GapFillFlag = 123,
    SecurityListRequestType = 559,
    NoMDEntryTypes = 267,
    TriggerPriceDirection = 1109,
    SelfTradeType = 7928,
    HandlInst = 21,
    SenderSubID = 50,
    StopLimitPx = 3040,
    CancelOrdersOnDisconnect = 8013,

    // Execution report tags
    OrderID = 37,
    ExecID = 17,
    ExecType = 150,
    OrdStatus = 39,
    LeavesQty = 151,
    CumQty = 14,
    AvgPx = 6,
    LastQty = 32,
    LastPx = 31,

    // Market data tags
    MDReqID = 262,
    SubscriptionRequestType = 263,
    MarketDepth = 264,
    MDUpdateType = 265,
    NoMDEntries = 268,
    MDEntryType = 269,
    MDEntryPx = 270,
    MDEntrySize = 271,
    MDEntryID = 278,
    MDUpdateAction = 279,

    // Security list tags
    SecurityReqID = 320,
    SecurityResponseID = 322,
    NoRelatedSym = 146,
    SecurityID = 48,
    SecurityIDSource = 22,
    SecurityType = 167,

    // Security status tags
    SecurityStatusReqID = 324,
    SecurityTradingStatus = 326,

    // Free text
    Text = 58,

    // Standard trailer
    CheckSum = 10,
}

impl Tag {
    /// Get the tag number as u32
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }

    /// Parse tag from u32
    pub fn from_u32(n: u32) -> Option<Self> {
        // This is a simple approach; for performance, could use a lookup table
        match n {
            8 => Some(Tag::BeginString),
            9 => Some(Tag::BodyLength),
            35 => Some(Tag::MsgType),
            34 => Some(Tag::MsgSeqNum),
            49 => Some(Tag::SenderCompID),
            52 => Some(Tag::SendingTime),
            56 => Some(Tag::TargetCompID),
            112 => Some(Tag::TestReqID),
            108 => Some(Tag::HeartBtInt),
            98 => Some(Tag::EncryptMethod),
            554 => Some(Tag::Password),
            553 => Some(Tag::Username),
            95 => Some(Tag::RawDataLength),
            96 => Some(Tag::RawData),
            141 => Some(Tag::ResetSeqNumFlag),
            7 => Some(Tag::BeginSeqNo),
            16 => Some(Tag::EndSeqNo),
            36 => Some(Tag::NewSeqNo),
            11 => Some(Tag::ClOrdID),
            55 => Some(Tag::Symbol),
            54 => Some(Tag::Side),
            38 => Some(Tag::OrderQty),
            40 => Some(Tag::OrdType),
            44 => Some(Tag::Price),
            59 => Some(Tag::TimeInForce),
            60 => Some(Tag::TransactTime),
            37 => Some(Tag::OrderID),
            17 => Some(Tag::ExecID),
            150 => Some(Tag::ExecType),
            39 => Some(Tag::OrdStatus),
            151 => Some(Tag::LeavesQty),
            14 => Some(Tag::CumQty),
            6 => Some(Tag::AvgPx),
            32 => Some(Tag::LastQty),
            31 => Some(Tag::LastPx),
            262 => Some(Tag::MDReqID),
            263 => Some(Tag::SubscriptionRequestType),
            264 => Some(Tag::MarketDepth),
            265 => Some(Tag::MDUpdateType),
            268 => Some(Tag::NoMDEntries),
            269 => Some(Tag::MDEntryType),
            270 => Some(Tag::MDEntryPx),
            271 => Some(Tag::MDEntrySize),
            278 => Some(Tag::MDEntryID),
            279 => Some(Tag::MDUpdateAction),
            320 => Some(Tag::SecurityReqID),
            322 => Some(Tag::SecurityResponseID),
            146 => Some(Tag::NoRelatedSym),
            48 => Some(Tag::SecurityID),
            22 => Some(Tag::SecurityIDSource),
            167 => Some(Tag::SecurityType),
            324 => Some(Tag::SecurityStatusReqID),
            326 => Some(Tag::SecurityTradingStatus),
            10 => Some(Tag::CheckSum),
            58 => Some(Tag::Text),
            _ => None,
        }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u32())
    }
}

/// Represents a parsed FIX message with tag-value pairs
#[derive(Debug, Clone)]
pub struct FixMessage {
    /// Message type
    pub msg_type: MsgType,
    /// Sequence number
    pub msg_seq_num: u64,
    /// Sender CompID
    pub sender_comp_id: String,
    /// Target CompID
    pub target_comp_id: String,
    /// Sending time (UTC timestamp in FIX format)
    pub sending_time: String,
    /// Additional fields as tag-value pairs
    pub fields: Vec<(Tag, String)>,
}

impl FixMessage {
    /// Create a new FixMessage
    pub fn new(
        msg_type: MsgType,
        msg_seq_num: u64,
        sender_comp_id: String,
        target_comp_id: String,
        sending_time: String,
    ) -> Self {
        Self {
            msg_type,
            msg_seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
            fields: Vec::new(),
        }
    }

    /// Add a field to the message
    pub fn add_field(&mut self, tag: Tag, value: String) {
        self.fields.push((tag, value));
    }

    /// Get a field value by tag
    pub fn get_field(&self, tag: Tag) -> Option<&str> {
        self.fields
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, v)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_type_conversion() {
        assert_eq!(MsgType::Logon.as_str(), "A");
        assert_eq!(MsgType::from_str("A"), Some(MsgType::Logon));

        assert_eq!(MsgType::NewOrderSingle.as_str(), "D");
        assert_eq!(MsgType::from_str("D"), Some(MsgType::NewOrderSingle));

        assert_eq!(MsgType::ExecutionReport.as_str(), "8");
        assert_eq!(MsgType::from_str("8"), Some(MsgType::ExecutionReport));

        assert_eq!(MsgType::from_str("INVALID"), None);
    }

    #[test]
    fn test_tag_conversion() {
        assert_eq!(Tag::BeginString.as_u32(), 8);
        assert_eq!(Tag::from_u32(8), Some(Tag::BeginString));

        assert_eq!(Tag::MsgType.as_u32(), 35);
        assert_eq!(Tag::from_u32(35), Some(Tag::MsgType));

        assert_eq!(Tag::CheckSum.as_u32(), 10);
        assert_eq!(Tag::from_u32(10), Some(Tag::CheckSum));

        assert_eq!(Tag::from_u32(99999), None);
    }

    #[test]
    fn test_fix_message_fields() {
        let mut msg = FixMessage::new(
            MsgType::Logon,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00".to_string(),
        );

        msg.add_field(Tag::HeartBtInt, "30".to_string());
        msg.add_field(Tag::EncryptMethod, "0".to_string());

        assert_eq!(msg.get_field(Tag::HeartBtInt), Some("30"));
        assert_eq!(msg.get_field(Tag::EncryptMethod), Some("0"));
        assert_eq!(msg.get_field(Tag::Username), None);
    }
}
