// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Core types for financial calculations using rust_decimal for precision
use rust_decimal::Decimal;

/// Currency amount (exact decimal representation)
pub type Amount = Decimal;

/// Exchange rate (exact decimal representation)
pub type Rate = Decimal;

/// Price (exact decimal representation)
pub type Price = Decimal;

/// Fee in basis points (1 bps = 0.01%)
pub type FeeBasisPoints = u16;

/// Currency symbol (e.g., "BTC", "USD", "ETH")
pub type CurrencySymbol = String;

/// Node index in the graph
pub type NodeIndex = usize;

/// Edge weight for graph algorithms (in -ln space)
pub type EdgeWeight = f64;
