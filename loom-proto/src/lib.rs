// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-proto
//!
//! Protocol buffer definitions for the Loom gRPC API.
//!
//! Generated via tonic-build at compile time from `.proto` files in `proto/`.
//! Defines the `LoomControl` service that loomctl uses to communicate with loom-daemon,
//! and all message types for querying engine status, listing pairs, discovering
//! opportunities, and streaming events.
//!
//! ## Examples
//!
//! ```ignore
//! use loom_proto::loom_control_client::LoomControlClient;
//! let client = LoomControlClient::connect("http://localhost:50051").await?;
//! let status = client.status(StatusRequest {}).await?;
//! ```

pub mod loom {
    tonic::include_proto!("loom");
}

pub use loom::*;
