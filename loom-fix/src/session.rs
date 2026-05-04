// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! FIX session layer implementation.
//!
//! This module will contain:
//! - Client-side session state management
//! - Server-side session state management (for test harness)
//! - HMAC-SHA256 authentication for Logon
//! - Heartbeat scheduling
//! - Sequence number management
//! - ResendRequest handling
//!
//! TODO: Implementation in Task 1.2 and 1.3

pub mod auth;
pub mod client;
pub mod server;

// Placeholder for now
