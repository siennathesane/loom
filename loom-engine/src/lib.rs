// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-engine
//!
//! Core arbitrage detection engine for the Loom cryptocurrency trading system.
//!
//! Consumes FIX market data, maintains a graph of exchange rates in -ln(rate) space,
//! runs Bellman-Ford cycle detection, and manages the lifecycle of arbitrage trades.
//! The event loop integrates FIX feeds, graph updates, and opportunity detection with
//! gravitational gating to filter low-profit cycles by a configurable safety margin.
//!
//! ## Examples
//!
//! ```ignore
//! let config = ListenConfig::default();
//! let loop_handle = ListenEventLoop::new(graph, store, config);
//! loop_handle.run(fix_rx).await;
//! ```

pub mod event_loop;
pub mod lump_sum;
pub mod state_machine;

pub use event_loop::{ListenConfig, ListenEventLoop, Opportunity};
pub use lump_sum::{Hop, LumpSum, LumpSumId, LumpSumState};
pub use state_machine::StateMachine;
