// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-fix
//!
//! Hand-rolled FIX 5.0 SP2 implementation for the Loom cryptocurrency arbitrage engine.
//!
//! This crate provides zero-allocation, high-performance FIX protocol handling with
//! a focus on correctness and the specific subset of FIX needed for Coinbase integration.
//!
//! ## Architecture
//!
//! - `message`: Core FIX message types and tag definitions
//! - `serialization`: Zero-allocation message serialization
//! - `parser`: State machine-based message parsing
//! - `session`: Session layer (client and server)
//! - `transport`: TLS transport with TCP_NODELAY

pub mod market_data;
pub mod message;
pub mod order_entry;
pub mod parser;
pub mod serialization;
pub mod session;
pub mod transport;

// Re-export commonly used types
pub use message::{FixMessage, MsgType, Tag};
pub use parser::{ParseError, Parser};
pub use serialization::{serialize_message, Serializer};

/// FIX field delimiter (SOH - Start of Header, ASCII 0x01)
pub const SOH: u8 = 0x01;

/// FIX version string for FIXT 1.1
pub const BEGIN_STRING: &str = "FIXT.1.1";
