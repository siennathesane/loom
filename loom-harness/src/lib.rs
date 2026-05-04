// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! FIX compliance test harness for Loom.
//!
//! This crate provides a mock Coinbase FIX server for testing the Loom
//! FIX client implementation. It simulates:
//!
//! - Session-level protocol (Logon, Heartbeat, TestRequest, etc.)
//! - Market data feeds (SecurityList, MarketDataIncrementalRefresh)
//! - Order entry (NewOrderSingle → ExecutionReport)
//!
//! The harness is used to validate FIX compliance before connecting to
//! the real Coinbase endpoints.

pub mod market_data;
pub mod order_book;
pub mod server;

pub use server::{MockCoinbaseServer, MockServerConfig};
