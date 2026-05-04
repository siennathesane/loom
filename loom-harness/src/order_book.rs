// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Order book simulator for test harness.

use loom_fix::message::{FixMessage, MsgType, Tag};
use loom_fix::serialization::build_fix_message;
use loom_fix::session::server::ServerSession;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Order execution result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionResult {
    /// Order filled at requested price
    Filled,
    /// Order rejected (e.g., insufficient funds)
    Rejected,
    /// Order cancelled
    Cancelled,
}

/// Simulated order
#[derive(Debug, Clone)]
struct Order {
    cl_ord_id: String,
    symbol: String,
    side: String,
    quantity: f64,
    price: f64,
    result: ExecutionResult,
}

/// Order book simulator
pub struct OrderBookSimulator {
    orders: HashMap<String, Order>,
    next_order_id: u64,
    /// Configurable behavior: should orders fill or reject?
    pub reject_orders: bool,
}

impl OrderBookSimulator {
    /// Create a new order book simulator
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            next_order_id: 1,
            reject_orders: false,
        }
    }

    /// Configure whether to reject orders (for testing rejection scenarios)
    pub fn set_reject_orders(&mut self, reject: bool) {
        self.reject_orders = reject;
    }

    /// Handle NewOrderSingle message
    pub fn handle_new_order(&mut self, session: &ServerSession, msg: &FixMessage) -> Vec<u8> {
        let cl_ord_id = msg.get_field(Tag::ClOrdID).unwrap_or("UNKNOWN").to_string();
        let symbol = msg.get_field(Tag::Symbol).unwrap_or("UNKNOWN").to_string();
        let side = msg.get_field(Tag::Side).unwrap_or("1").to_string();
        let quantity = msg
            .get_field(Tag::OrderQty)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let price = msg
            .get_field(Tag::Price)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let order_id = self.next_order_id;
        self.next_order_id += 1;

        let result = if self.reject_orders {
            ExecutionResult::Rejected
        } else {
            ExecutionResult::Filled
        };

        let order = Order {
            cl_ord_id: cl_ord_id.clone(),
            symbol: symbol.clone(),
            side: side.clone(),
            quantity,
            price,
            result,
        };

        self.orders.insert(cl_ord_id.clone(), order);

        // Create ExecutionReport
        self.create_execution_report(
            session,
            &cl_ord_id,
            &format!("ORD_{}", order_id),
            &format!("EXEC_{}", order_id),
            &symbol,
            &side,
            quantity,
            price,
            result,
        )
    }

    /// Handle OrderCancelRequest message
    pub fn handle_cancel_request(&mut self, session: &ServerSession, msg: &FixMessage) -> Vec<u8> {
        let cl_ord_id = msg.get_field(Tag::ClOrdID).unwrap_or("UNKNOWN").to_string();
        // For now, we use ClOrdID to look up the order (simplified)
        // In real FIX, we'd use OrigClOrdID tag (41)

        // For simplicity, assume cancel succeeds - look for ANY matching order
        let order_key = self.orders.keys().next().cloned();

        if let Some(key) = order_key {
            if let Some(order) = self.orders.get(&key) {
                let symbol = order.symbol.clone();
                let side = order.side.clone();
                let quantity = order.quantity;
                let price = order.price;

                // Update order state
                if let Some(order) = self.orders.get_mut(&key) {
                    order.result = ExecutionResult::Cancelled;
                }

                self.create_execution_report(
                    session,
                    &cl_ord_id,
                    &format!("ORD_{}", self.next_order_id),
                    &format!("EXEC_{}", self.next_order_id),
                    &symbol,
                    &side,
                    quantity,
                    price,
                    ExecutionResult::Cancelled,
                )
            } else {
                // Order not found - send OrderCancelReject
                self.create_cancel_reject(session, &cl_ord_id, &cl_ord_id)
            }
        } else {
            // No orders - send OrderCancelReject
            self.create_cancel_reject(session, &cl_ord_id, &cl_ord_id)
        }
    }

    /// Simulate an order fill (for testing)
    pub fn simulate_fill(&mut self, order_id: &str, filled: bool) {
        if let Some(order) = self.orders.get_mut(order_id) {
            order.result = if filled {
                ExecutionResult::Filled
            } else {
                ExecutionResult::Rejected
            };
        }
    }

    /// Create ExecutionReport message
    fn create_execution_report(
        &self,
        session: &ServerSession,
        cl_ord_id: &str,
        order_id: &str,
        exec_id: &str,
        symbol: &str,
        side: &str,
        quantity: f64,
        price: f64,
        result: ExecutionResult,
    ) -> Vec<u8> {
        let seq_num = session.current_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::ExecutionReport,
            seq_num,
            "Coinbase".to_string(),
            "CLIENT".to_string(),
            sending_time,
        );

        msg.add_field(Tag::ClOrdID, cl_ord_id.to_string());
        msg.add_field(Tag::OrderID, order_id.to_string());
        msg.add_field(Tag::ExecID, exec_id.to_string());
        msg.add_field(Tag::Symbol, symbol.to_string());
        msg.add_field(Tag::Side, side.to_string());

        match result {
            ExecutionResult::Filled => {
                msg.add_field(Tag::ExecType, "F".to_string()); // Fill
                msg.add_field(Tag::OrdStatus, "2".to_string()); // Filled
                msg.add_field(Tag::LastQty, quantity.to_string());
                msg.add_field(Tag::LastPx, price.to_string());
                msg.add_field(Tag::CumQty, quantity.to_string());
                msg.add_field(Tag::AvgPx, price.to_string());
                msg.add_field(Tag::LeavesQty, "0".to_string());
            }
            ExecutionResult::Rejected => {
                msg.add_field(Tag::ExecType, "8".to_string()); // Rejected
                msg.add_field(Tag::OrdStatus, "8".to_string()); // Rejected
                msg.add_field(Tag::CumQty, "0".to_string());
                msg.add_field(Tag::LeavesQty, "0".to_string());
            }
            ExecutionResult::Cancelled => {
                msg.add_field(Tag::ExecType, "4".to_string()); // Cancelled
                msg.add_field(Tag::OrdStatus, "4".to_string()); // Cancelled
                msg.add_field(Tag::CumQty, "0".to_string());
                msg.add_field(Tag::LeavesQty, "0".to_string());
            }
        }

        build_fix_message(&msg)
    }

    /// Create OrderCancelReject message
    fn create_cancel_reject(
        &self,
        session: &ServerSession,
        cl_ord_id: &str,
        orig_cl_ord_id: &str,
    ) -> Vec<u8> {
        let seq_num = session.current_seq_num_out();
        let sending_time = Self::fix_timestamp();

        let mut msg = FixMessage::new(
            MsgType::OrderCancelReject,
            seq_num,
            "Coinbase".to_string(),
            "CLIENT".to_string(),
            sending_time,
        );

        msg.add_field(Tag::ClOrdID, cl_ord_id.to_string());
        msg.add_field(Tag::OrderID, orig_cl_ord_id.to_string());

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

impl Default for OrderBookSimulator {
    fn default() -> Self {
        Self::new()
    }
}
