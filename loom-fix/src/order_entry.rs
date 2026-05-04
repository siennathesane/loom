// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Order entry message types and builders.
//!
//! This module provides structures and functions for working with FIX order entry messages,
//! including NewOrderSingle, ExecutionReport, OrderCancelRequest, and OrderCancelReject.
//!
//! Key features:
//! - Exact decimal arithmetic for all quantities and prices
//! - Support for all order states and execution types
//! - IOC (Immediate-Or-Cancel) limit orders for Coinbase

use crate::message::{FixMessage, MsgType, Tag};
use crate::serialization::build_fix_message;
use rust_decimal::Decimal;
use std::str::FromStr;

/// Order side (Buy/Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    /// Buy (1)
    Buy,
    /// Sell (2)
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSide::Buy => "1",
            OrderSide::Sell => "2",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1" => Some(OrderSide::Buy),
            "2" => Some(OrderSide::Sell),
            _ => None,
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    /// Market (1)
    Market,
    /// Limit (2)
    Limit,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Market => "1",
            OrderType::Limit => "2",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1" => Some(OrderType::Market),
            "2" => Some(OrderType::Limit),
            _ => None,
        }
    }
}

/// Time in force
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    /// Day (0)
    Day,
    /// Good Till Cancel (1)
    GoodTillCancel,
    /// Immediate Or Cancel (3)
    ImmediateOrCancel,
    /// Fill Or Kill (4)
    FillOrKill,
}

impl TimeInForce {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeInForce::Day => "0",
            TimeInForce::GoodTillCancel => "1",
            TimeInForce::ImmediateOrCancel => "3",
            TimeInForce::FillOrKill => "4",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(TimeInForce::Day),
            "1" => Some(TimeInForce::GoodTillCancel),
            "3" => Some(TimeInForce::ImmediateOrCancel),
            "4" => Some(TimeInForce::FillOrKill),
            _ => None,
        }
    }
}

/// Execution type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecType {
    /// New (0)
    New,
    /// Partial fill (1)
    PartialFill,
    /// Fill (2)
    Fill,
    /// Canceled (4)
    Canceled,
    /// Rejected (8)
    Rejected,
}

impl ExecType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecType::New => "0",
            ExecType::PartialFill => "1",
            ExecType::Fill => "2",
            ExecType::Canceled => "4",
            ExecType::Rejected => "8",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(ExecType::New),
            "1" => Some(ExecType::PartialFill),
            "2" => Some(ExecType::Fill),
            "4" => Some(ExecType::Canceled),
            "8" => Some(ExecType::Rejected),
            _ => None,
        }
    }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdStatus {
    /// New (0)
    New,
    /// Partially filled (1)
    PartiallyFilled,
    /// Filled (2)
    Filled,
    /// Canceled (4)
    Canceled,
    /// Rejected (8)
    Rejected,
}

impl OrdStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrdStatus::New => "0",
            OrdStatus::PartiallyFilled => "1",
            OrdStatus::Filled => "2",
            OrdStatus::Canceled => "4",
            OrdStatus::Rejected => "8",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(OrdStatus::New),
            "1" => Some(OrdStatus::PartiallyFilled),
            "2" => Some(OrdStatus::Filled),
            "4" => Some(OrdStatus::Canceled),
            "8" => Some(OrdStatus::Rejected),
            _ => None,
        }
    }
}

/// Builder for NewOrderSingle messages
pub struct NewOrderSingleBuilder {
    msg: FixMessage,
    cl_ord_id: Option<String>,
    symbol: Option<String>,
    side: Option<OrderSide>,
    order_qty: Option<Decimal>,
    ord_type: Option<OrderType>,
    price: Option<Decimal>,
    time_in_force: Option<TimeInForce>,
}

impl NewOrderSingleBuilder {
    /// Create a new NewOrderSingle builder
    pub fn new(
        seq_num: u64,
        sender_comp_id: String,
        target_comp_id: String,
        sending_time: String,
    ) -> Self {
        let msg = FixMessage::new(
            MsgType::NewOrderSingle,
            seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
        );

        Self {
            msg,
            cl_ord_id: None,
            symbol: None,
            side: None,
            order_qty: None,
            ord_type: None,
            price: None,
            time_in_force: None,
        }
    }

    /// Set client order ID
    pub fn cl_ord_id(mut self, cl_ord_id: String) -> Self {
        self.cl_ord_id = Some(cl_ord_id);
        self
    }

    /// Set symbol
    pub fn symbol(mut self, symbol: String) -> Self {
        self.symbol = Some(symbol);
        self
    }

    /// Set order side
    pub fn side(mut self, side: OrderSide) -> Self {
        self.side = Some(side);
        self
    }

    /// Set order quantity
    pub fn order_qty(mut self, qty: Decimal) -> Self {
        self.order_qty = Some(qty);
        self
    }

    /// Set order type
    pub fn ord_type(mut self, ord_type: OrderType) -> Self {
        self.ord_type = Some(ord_type);
        self
    }

    /// Set limit price
    pub fn price(mut self, price: Decimal) -> Self {
        self.price = Some(price);
        self
    }

    /// Set time in force
    pub fn time_in_force(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = Some(tif);
        self
    }

    /// Build the FIX message bytes
    pub fn build(mut self) -> Vec<u8> {
        // Add required fields
        if let Some(cl_ord_id) = self.cl_ord_id {
            self.msg.add_field(Tag::ClOrdID, cl_ord_id);
        }
        if let Some(symbol) = self.symbol {
            self.msg.add_field(Tag::Symbol, symbol);
        }
        if let Some(side) = self.side {
            self.msg.add_field(Tag::Side, side.as_str().to_string());
        }
        if let Some(qty) = self.order_qty {
            self.msg.add_field(Tag::OrderQty, qty.to_string());
        }
        if let Some(ord_type) = self.ord_type {
            self.msg
                .add_field(Tag::OrdType, ord_type.as_str().to_string());
        }
        if let Some(price) = self.price {
            self.msg.add_field(Tag::Price, price.to_string());
        }
        if let Some(tif) = self.time_in_force {
            self.msg
                .add_field(Tag::TimeInForce, tif.as_str().to_string());
        }

        build_fix_message(&self.msg)
    }
}

/// Parsed ExecutionReport message
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionReport {
    /// Client order ID
    pub cl_ord_id: String,
    /// Exchange order ID
    pub order_id: String,
    /// Execution type
    pub exec_type: ExecType,
    /// Order status
    pub ord_status: OrdStatus,
    /// Symbol
    pub symbol: String,
    /// Order side
    pub side: OrderSide,
    /// Order quantity
    pub order_qty: Decimal,
    /// Cumulative quantity filled
    pub cum_qty: Decimal,
    /// Remaining quantity
    pub leaves_qty: Decimal,
    /// Last fill quantity (optional)
    pub last_qty: Option<Decimal>,
    /// Last fill price (optional)
    pub last_px: Option<Decimal>,
    /// Average fill price (optional)
    pub avg_px: Option<Decimal>,
}

impl ExecutionReport {
    /// Parse an ExecutionReport from a FixMessage
    pub fn from_fix_message(msg: &FixMessage) -> Option<Self> {
        if msg.msg_type != MsgType::ExecutionReport {
            return None;
        }

        let cl_ord_id = msg.get_field(Tag::ClOrdID)?.to_string();
        let order_id = msg.get_field(Tag::OrderID)?.to_string();

        let exec_type_str = msg.get_field(Tag::ExecType)?;
        let exec_type = ExecType::from_str(exec_type_str)?;

        let ord_status_str = msg.get_field(Tag::OrdStatus)?;
        let ord_status = OrdStatus::from_str(ord_status_str)?;

        let symbol = msg.get_field(Tag::Symbol)?.to_string();

        let side_str = msg.get_field(Tag::Side)?;
        let side = OrderSide::from_str(side_str)?;

        let order_qty = Decimal::from_str(msg.get_field(Tag::OrderQty)?).ok()?;
        let cum_qty = Decimal::from_str(msg.get_field(Tag::CumQty)?).ok()?;
        let leaves_qty = Decimal::from_str(msg.get_field(Tag::LeavesQty)?).ok()?;

        let last_qty = msg
            .get_field(Tag::LastQty)
            .and_then(|s| Decimal::from_str(s).ok());
        let last_px = msg
            .get_field(Tag::LastPx)
            .and_then(|s| Decimal::from_str(s).ok());
        let avg_px = msg
            .get_field(Tag::AvgPx)
            .and_then(|s| Decimal::from_str(s).ok());

        Some(Self {
            cl_ord_id,
            order_id,
            exec_type,
            ord_status,
            symbol,
            side,
            order_qty,
            cum_qty,
            leaves_qty,
            last_qty,
            last_px,
            avg_px,
        })
    }
}

/// Builder for OrderCancelRequest messages
pub struct OrderCancelRequestBuilder {
    msg: FixMessage,
}

impl OrderCancelRequestBuilder {
    /// Create a new OrderCancelRequest builder
    pub fn new(
        seq_num: u64,
        sender_comp_id: String,
        target_comp_id: String,
        sending_time: String,
        _orig_cl_ord_id: String,
        cl_ord_id: String,
        symbol: String,
        side: OrderSide,
    ) -> Self {
        let mut msg = FixMessage::new(
            MsgType::OrderCancelRequest,
            seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
        );

        msg.add_field(Tag::ClOrdID, cl_ord_id);
        msg.add_field(Tag::Symbol, symbol);
        msg.add_field(Tag::Side, side.as_str().to_string());

        Self { msg }
    }

    /// Build the FIX message bytes
    pub fn build(self) -> Vec<u8> {
        build_fix_message(&self.msg)
    }
}

/// Parsed OrderCancelReject message
#[derive(Debug, Clone, PartialEq)]
pub struct OrderCancelReject {
    /// Client order ID
    pub cl_ord_id: String,
    /// Original client order ID
    pub orig_cl_ord_id: String,
    /// Order status
    pub ord_status: OrdStatus,
}

impl OrderCancelReject {
    /// Parse an OrderCancelReject from a FixMessage
    pub fn from_fix_message(msg: &FixMessage) -> Option<Self> {
        if msg.msg_type != MsgType::OrderCancelReject {
            return None;
        }

        let cl_ord_id = msg.get_field(Tag::ClOrdID)?.to_string();
        let orig_cl_ord_id = msg.get_field(Tag::ClOrdID)?.to_string();

        let ord_status_str = msg.get_field(Tag::OrdStatus)?;
        let ord_status = OrdStatus::from_str(ord_status_str)?;

        Some(Self {
            cl_ord_id,
            orig_cl_ord_id,
            ord_status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_side_conversion() {
        assert_eq!(OrderSide::Buy.as_str(), "1");
        assert_eq!(OrderSide::Sell.as_str(), "2");
        assert_eq!(OrderSide::from_str("1"), Some(OrderSide::Buy));
        assert_eq!(OrderSide::from_str("2"), Some(OrderSide::Sell));
    }

    #[test]
    fn test_order_type_conversion() {
        assert_eq!(OrderType::Market.as_str(), "1");
        assert_eq!(OrderType::Limit.as_str(), "2");
        assert_eq!(OrderType::from_str("2"), Some(OrderType::Limit));
    }

    #[test]
    fn test_time_in_force_conversion() {
        assert_eq!(TimeInForce::ImmediateOrCancel.as_str(), "3");
        assert_eq!(
            TimeInForce::from_str("3"),
            Some(TimeInForce::ImmediateOrCancel)
        );
    }

    #[test]
    fn test_exec_type_conversion() {
        assert_eq!(ExecType::New.as_str(), "0");
        assert_eq!(ExecType::Fill.as_str(), "2");
        assert_eq!(ExecType::from_str("2"), Some(ExecType::Fill));
    }

    #[test]
    fn test_ord_status_conversion() {
        assert_eq!(OrdStatus::New.as_str(), "0");
        assert_eq!(OrdStatus::Filled.as_str(), "2");
        assert_eq!(OrdStatus::from_str("2"), Some(OrdStatus::Filled));
    }

    #[test]
    fn test_new_order_single_builder() {
        let order = NewOrderSingleBuilder::new(
            1,
            "CLIENT".to_string(),
            "COINBASE".to_string(),
            "20260212-12:00:00.000".to_string(),
        )
        .cl_ord_id("TEST-001".to_string())
        .symbol("BTC-USD".to_string())
        .side(OrderSide::Buy)
        .order_qty(Decimal::from_str("0.1").unwrap())
        .ord_type(OrderType::Limit)
        .price(Decimal::from_str("50000.00").unwrap())
        .time_in_force(TimeInForce::ImmediateOrCancel)
        .build();

        // Parse it back
        use crate::parser::Parser;
        let mut parser = Parser::new();
        parser.feed(&order);
        let parsed = parser.parse_message().unwrap().unwrap();

        assert_eq!(parsed.msg_type, MsgType::NewOrderSingle);
        assert_eq!(parsed.get_field(Tag::ClOrdID), Some("TEST-001"));
        assert_eq!(parsed.get_field(Tag::Symbol), Some("BTC-USD"));
        assert_eq!(parsed.get_field(Tag::Side), Some("1")); // Buy
        assert_eq!(parsed.get_field(Tag::OrderQty), Some("0.1"));
        assert_eq!(parsed.get_field(Tag::OrdType), Some("2")); // Limit
        assert_eq!(parsed.get_field(Tag::Price), Some("50000.00"));
        assert_eq!(parsed.get_field(Tag::TimeInForce), Some("3")); // IOC
    }

    #[test]
    fn test_execution_report_parsing_new() {
        let mut msg = FixMessage::new(
            MsgType::ExecutionReport,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::ClOrdID, "TEST-001".to_string());
        msg.add_field(Tag::OrderID, "EXCH-123".to_string());
        msg.add_field(Tag::ExecType, "0".to_string()); // New
        msg.add_field(Tag::OrdStatus, "0".to_string()); // New
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string()); // Buy
        msg.add_field(Tag::OrderQty, "0.1".to_string());
        msg.add_field(Tag::CumQty, "0".to_string());
        msg.add_field(Tag::LeavesQty, "0.1".to_string());

        let report = ExecutionReport::from_fix_message(&msg).unwrap();
        assert_eq!(report.cl_ord_id, "TEST-001");
        assert_eq!(report.order_id, "EXCH-123");
        assert_eq!(report.exec_type, ExecType::New);
        assert_eq!(report.ord_status, OrdStatus::New);
        assert_eq!(report.cum_qty, Decimal::from_str("0").unwrap());
        assert_eq!(report.leaves_qty, Decimal::from_str("0.1").unwrap());
    }

    #[test]
    fn test_execution_report_parsing_fill() {
        let mut msg = FixMessage::new(
            MsgType::ExecutionReport,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::ClOrdID, "TEST-001".to_string());
        msg.add_field(Tag::OrderID, "EXCH-123".to_string());
        msg.add_field(Tag::ExecType, "2".to_string()); // Fill
        msg.add_field(Tag::OrdStatus, "2".to_string()); // Filled
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::OrderQty, "0.1".to_string());
        msg.add_field(Tag::CumQty, "0.1".to_string());
        msg.add_field(Tag::LeavesQty, "0".to_string());
        msg.add_field(Tag::LastQty, "0.1".to_string());
        msg.add_field(Tag::LastPx, "50000.00".to_string());
        msg.add_field(Tag::AvgPx, "50000.00".to_string());

        let report = ExecutionReport::from_fix_message(&msg).unwrap();
        assert_eq!(report.exec_type, ExecType::Fill);
        assert_eq!(report.ord_status, OrdStatus::Filled);
        assert_eq!(report.cum_qty, Decimal::from_str("0.1").unwrap());
        assert_eq!(report.leaves_qty, Decimal::from_str("0").unwrap());
        assert_eq!(report.last_qty, Some(Decimal::from_str("0.1").unwrap()));
        assert_eq!(report.last_px, Some(Decimal::from_str("50000.00").unwrap()));
        assert_eq!(report.avg_px, Some(Decimal::from_str("50000.00").unwrap()));
    }

    #[test]
    fn test_execution_report_parsing_partial_fill() {
        let mut msg = FixMessage::new(
            MsgType::ExecutionReport,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::ClOrdID, "TEST-001".to_string());
        msg.add_field(Tag::OrderID, "EXCH-123".to_string());
        msg.add_field(Tag::ExecType, "1".to_string()); // PartialFill
        msg.add_field(Tag::OrdStatus, "1".to_string()); // PartiallyFilled
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::OrderQty, "0.1".to_string());
        msg.add_field(Tag::CumQty, "0.05".to_string());
        msg.add_field(Tag::LeavesQty, "0.05".to_string());
        msg.add_field(Tag::LastQty, "0.05".to_string());
        msg.add_field(Tag::LastPx, "50000.00".to_string());
        msg.add_field(Tag::AvgPx, "50000.00".to_string());

        let report = ExecutionReport::from_fix_message(&msg).unwrap();
        assert_eq!(report.exec_type, ExecType::PartialFill);
        assert_eq!(report.ord_status, OrdStatus::PartiallyFilled);
        assert_eq!(report.cum_qty, Decimal::from_str("0.05").unwrap());
        assert_eq!(report.leaves_qty, Decimal::from_str("0.05").unwrap());
    }

    #[test]
    fn test_execution_report_parsing_rejected() {
        let mut msg = FixMessage::new(
            MsgType::ExecutionReport,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::ClOrdID, "TEST-001".to_string());
        msg.add_field(Tag::OrderID, "NONE".to_string());
        msg.add_field(Tag::ExecType, "8".to_string()); // Rejected
        msg.add_field(Tag::OrdStatus, "8".to_string()); // Rejected
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::OrderQty, "0.1".to_string());
        msg.add_field(Tag::CumQty, "0".to_string());
        msg.add_field(Tag::LeavesQty, "0".to_string());

        let report = ExecutionReport::from_fix_message(&msg).unwrap();
        assert_eq!(report.exec_type, ExecType::Rejected);
        assert_eq!(report.ord_status, OrdStatus::Rejected);
        assert_eq!(report.cum_qty, Decimal::from_str("0").unwrap());
    }

    #[test]
    fn test_execution_report_parsing_canceled() {
        let mut msg = FixMessage::new(
            MsgType::ExecutionReport,
            1,
            "COINBASE".to_string(),
            "CLIENT".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::ClOrdID, "TEST-001".to_string());
        msg.add_field(Tag::OrderID, "EXCH-123".to_string());
        msg.add_field(Tag::ExecType, "4".to_string()); // Canceled
        msg.add_field(Tag::OrdStatus, "4".to_string()); // Canceled
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string());
        msg.add_field(Tag::OrderQty, "0.1".to_string());
        msg.add_field(Tag::CumQty, "0.05".to_string());
        msg.add_field(Tag::LeavesQty, "0".to_string());

        let report = ExecutionReport::from_fix_message(&msg).unwrap();
        assert_eq!(report.exec_type, ExecType::Canceled);
        assert_eq!(report.ord_status, OrdStatus::Canceled);
        assert_eq!(report.cum_qty, Decimal::from_str("0.05").unwrap());
        assert_eq!(report.leaves_qty, Decimal::from_str("0").unwrap());
    }
}
