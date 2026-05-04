// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Adjacency matrix for the cryptocurrency exchange rate graph
///
/// Uses Structure-of-Arrays (SOA) layout for cache efficiency.
/// Stores edge weights as -ln(rate × (1 - fee)) for Bellman-Ford algorithm.
///
/// Memory layout: Row-major, size × size matrices
/// Target: Fit in L1 cache (32KB) for typical graph sizes (~200 currencies)
use crate::types::*;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Perfect hash mapping currency symbols to matrix indices
#[derive(Debug, Clone)]
pub struct CurrencyHash {
    symbol_to_index: HashMap<CurrencySymbol, NodeIndex>,
    index_to_symbol: Vec<CurrencySymbol>,
}

impl CurrencyHash {
    /// Create a new currency hash from a list of symbols
    pub fn new(currencies: Vec<CurrencySymbol>) -> Self {
        let mut symbol_to_index = HashMap::new();
        let mut index_to_symbol = Vec::new();

        for (idx, symbol) in currencies.into_iter().enumerate() {
            symbol_to_index.insert(symbol.clone(), idx);
            index_to_symbol.push(symbol);
        }

        Self {
            symbol_to_index,
            index_to_symbol,
        }
    }

    /// Get the index for a currency symbol
    pub fn get_index(&self, symbol: &str) -> Option<NodeIndex> {
        self.symbol_to_index.get(symbol).copied()
    }

    /// Get the currency symbol for an index
    pub fn get_symbol(&self, index: NodeIndex) -> Option<&str> {
        self.index_to_symbol.get(index).map(|s| s.as_str())
    }

    /// Number of currencies
    pub fn size(&self) -> usize {
        self.index_to_symbol.len()
    }

    /// Check if a symbol exists
    pub fn contains(&self, symbol: &str) -> bool {
        self.symbol_to_index.contains_key(symbol)
    }
}

/// Adjacency matrix representing the exchange rate graph
///
/// SOA layout: weights and rates stored in separate flat arrays
/// Row-major indexing: matrix[i][j] is stored at array[i * size + j]
#[derive(Debug, Clone)]
pub struct AdjacencyMatrix {
    /// Number of currencies (nodes)
    size: usize,

    /// Edge weights: -ln(exchange_rate × (1 - fee))
    /// Used by Bellman-Ford for cycle detection
    /// f64::INFINITY represents no edge
    weights: Vec<EdgeWeight>,

    /// Original exchange rates (exact decimal)
    /// Used for final profit calculations and verification
    /// Decimal::ZERO represents no edge
    rates: Vec<Rate>,

    /// Currency symbol to index mapping
    currency_hash: CurrencyHash,
}

impl AdjacencyMatrix {
    /// Create a new adjacency matrix for the given currencies
    ///
    /// All edges are initially absent (weight = INFINITY, rate = 0)
    pub fn new(currencies: Vec<CurrencySymbol>) -> Self {
        let currency_hash = CurrencyHash::new(currencies);
        let size = currency_hash.size();
        let capacity = size * size;

        Self {
            size,
            weights: vec![f64::INFINITY; capacity],
            rates: vec![Decimal::ZERO; capacity],
            currency_hash,
        }
    }

    /// Get the number of nodes in the graph
    pub fn size(&self) -> usize {
        self.size
    }

    /// Convert (from, to) indices to flat array index
    #[inline]
    fn to_flat_index(&self, from_idx: NodeIndex, to_idx: NodeIndex) -> usize {
        from_idx * self.size + to_idx
    }

    /// Update an edge with a new exchange rate and fee
    ///
    /// # Arguments
    /// * `from` - Source currency symbol
    /// * `to` - Destination currency symbol
    /// * `rate` - Exchange rate (how much `to` currency you get for 1 unit of `from`)
    /// * `fee_bps` - Trading fee in basis points (e.g., 60 for 0.60%)
    ///
    /// # Edge Weight Calculation
    /// weight = -ln(rate × (1 - fee_bps/10000))
    ///
    /// This transformation converts multiplicative exchange rates into additive
    /// edge weights, allowing Bellman-Ford to find profitable cycles:
    /// - Negative weight cycle = product of rates > 1 = arbitrage opportunity
    pub fn update_edge(
        &mut self,
        from: &str,
        to: &str,
        rate: Rate,
        fee_bps: FeeBasisPoints,
    ) -> Result<(), String> {
        let from_idx = self
            .currency_hash
            .get_index(from)
            .ok_or_else(|| format!("Currency not found: {}", from))?;
        let to_idx = self
            .currency_hash
            .get_index(to)
            .ok_or_else(|| format!("Currency not found: {}", to))?;

        // Calculate rate after fee: rate × (1 - fee_bps/10000)
        let fee_decimal = Decimal::new(fee_bps as i64, 4); // 0.0060 for 60 bps
        let rate_after_fee = rate * (Decimal::ONE - fee_decimal);

        // Calculate edge weight: -ln(rate_after_fee)
        // Convert to f64 for ln() calculation
        let rate_f64 = rate_after_fee
            .to_f64()
            .ok_or_else(|| format!("Failed to convert rate to f64: {}", rate_after_fee))?;

        if rate_f64 <= 0.0 {
            return Err(format!("Invalid rate (must be positive): {}", rate_f64));
        }

        let weight = -rate_f64.ln();

        // Update both weights and rates
        let idx = self.to_flat_index(from_idx, to_idx);
        self.weights[idx] = weight;
        self.rates[idx] = rate;

        Ok(())
    }

    /// Remove an edge (set to infinity/zero)
    pub fn remove_edge(&mut self, from: &str, to: &str) -> Result<(), String> {
        let from_idx = self
            .currency_hash
            .get_index(from)
            .ok_or_else(|| format!("Currency not found: {}", from))?;
        let to_idx = self
            .currency_hash
            .get_index(to)
            .ok_or_else(|| format!("Currency not found: {}", to))?;

        let idx = self.to_flat_index(from_idx, to_idx);
        self.weights[idx] = f64::INFINITY;
        self.rates[idx] = Decimal::ZERO;

        Ok(())
    }

    /// Get the edge weight between two nodes (by index)
    pub fn get_weight(&self, from_idx: NodeIndex, to_idx: NodeIndex) -> EdgeWeight {
        let idx = self.to_flat_index(from_idx, to_idx);
        self.weights[idx]
    }

    /// Get the exchange rate between two nodes (by index)
    pub fn get_rate(&self, from_idx: NodeIndex, to_idx: NodeIndex) -> Rate {
        let idx = self.to_flat_index(from_idx, to_idx);
        self.rates[idx]
    }

    /// Get the edge weight between two currencies (by symbol)
    pub fn get_weight_by_symbol(&self, from: &str, to: &str) -> Option<EdgeWeight> {
        let from_idx = self.currency_hash.get_index(from)?;
        let to_idx = self.currency_hash.get_index(to)?;
        Some(self.get_weight(from_idx, to_idx))
    }

    /// Get the exchange rate between two currencies (by symbol)
    pub fn get_rate_by_symbol(&self, from: &str, to: &str) -> Option<Rate> {
        let from_idx = self.currency_hash.get_index(from)?;
        let to_idx = self.currency_hash.get_index(to)?;
        Some(self.get_rate(from_idx, to_idx))
    }

    /// Check if an edge exists
    pub fn has_edge(&self, from_idx: NodeIndex, to_idx: NodeIndex) -> bool {
        let weight = self.get_weight(from_idx, to_idx);
        weight.is_finite()
    }

    /// Get currency symbol for a node index
    pub fn get_symbol(&self, index: NodeIndex) -> Option<&str> {
        self.currency_hash.get_symbol(index)
    }

    /// Get node index for a currency symbol
    pub fn get_index(&self, symbol: &str) -> Option<NodeIndex> {
        self.currency_hash.get_index(symbol)
    }

    /// Get all neighbors of a node (outgoing edges)
    pub fn neighbors(&self, from_idx: NodeIndex) -> Vec<NodeIndex> {
        (0..self.size)
            .filter(|&to_idx| self.has_edge(from_idx, to_idx))
            .collect()
    }

    /// Calculate memory footprint in bytes
    pub fn memory_size(&self) -> usize {
        let weights_size = self.weights.len() * std::mem::size_of::<EdgeWeight>();
        let rates_size = self.rates.len() * std::mem::size_of::<Rate>();
        let hash_size = self.currency_hash.symbol_to_index.len()
            * (std::mem::size_of::<String>() + std::mem::size_of::<NodeIndex>());

        weights_size + rates_size + hash_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_currency_hash_basic() {
        let currencies = vec!["BTC".to_string(), "ETH".to_string(), "USD".to_string()];
        let hash = CurrencyHash::new(currencies);

        assert_eq!(hash.size(), 3);
        assert_eq!(hash.get_index("BTC"), Some(0));
        assert_eq!(hash.get_index("ETH"), Some(1));
        assert_eq!(hash.get_index("USD"), Some(2));
        assert_eq!(hash.get_index("INVALID"), None);

        assert_eq!(hash.get_symbol(0), Some("BTC"));
        assert_eq!(hash.get_symbol(1), Some("ETH"));
        assert_eq!(hash.get_symbol(2), Some("USD"));
        assert_eq!(hash.get_symbol(3), None);
    }

    #[test]
    fn test_adjacency_matrix_creation() {
        let currencies = vec!["BTC".to_string(), "USD".to_string()];
        let matrix = AdjacencyMatrix::new(currencies);

        assert_eq!(matrix.size(), 2);

        // All edges should be absent initially
        assert_eq!(matrix.get_weight(0, 0), f64::INFINITY);
        assert_eq!(matrix.get_weight(0, 1), f64::INFINITY);
        assert_eq!(matrix.get_weight(1, 0), f64::INFINITY);
        assert_eq!(matrix.get_weight(1, 1), f64::INFINITY);
    }

    #[test]
    fn test_update_edge() {
        let currencies = vec!["BTC".to_string(), "USD".to_string()];
        let mut matrix = AdjacencyMatrix::new(currencies);

        // BTC -> USD: 1 BTC = 50000 USD, 0.6% fee
        let rate = Decimal::new(50000, 0);
        let fee_bps = 60;

        matrix.update_edge("BTC", "USD", rate, fee_bps).unwrap();

        // Check that edge exists
        assert!(matrix.has_edge(0, 1));

        // Check that rate is stored
        assert_eq!(matrix.get_rate(0, 1), rate);

        // Check that weight is calculated correctly
        // weight = -ln(50000 × 0.994) = -ln(49700)
        let expected_weight = -(50000.0 * 0.994_f64).ln();
        let actual_weight = matrix.get_weight(0, 1);
        assert!((actual_weight - expected_weight).abs() < 1e-10);
    }

    #[test]
    fn test_bidirectional_edges() {
        let currencies = vec!["BTC".to_string(), "USD".to_string()];
        let mut matrix = AdjacencyMatrix::new(currencies);

        // BTC -> USD: 1 BTC = 50000 USD
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();

        // USD -> BTC: 1 USD = 0.00002 BTC (inverse rate)
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();

        // Both edges should exist independently
        assert!(matrix.has_edge(0, 1));
        assert!(matrix.has_edge(1, 0));

        // Weights should satisfy: weight(A→B) + weight(B→A) ≈ 0 (for same fees)
        // This is because -ln(a) + -ln(1/a) = -ln(a) - ln(a) = -2ln(a) for exact inverses
        // But with fees, they're not exact inverses
        let weight_btc_usd = matrix.get_weight(0, 1);
        let weight_usd_btc = matrix.get_weight(1, 0);

        // Both should be finite
        assert!(weight_btc_usd.is_finite());
        assert!(weight_usd_btc.is_finite());
    }

    #[test]
    fn test_fee_calculation() {
        let currencies = vec!["BTC".to_string(), "USD".to_string()];
        let mut matrix = AdjacencyMatrix::new(currencies);

        // Test with different fee levels
        let rate = Decimal::new(50000, 0);

        // 0% fee
        matrix.update_edge("BTC", "USD", rate, 0).unwrap();
        let weight_no_fee = matrix.get_weight(0, 1);
        let expected_no_fee = -50000.0_f64.ln();
        assert!((weight_no_fee - expected_no_fee).abs() < 1e-10);

        // 0.6% fee (60 bps)
        matrix.update_edge("BTC", "USD", rate, 60).unwrap();
        let weight_with_fee = matrix.get_weight(0, 1);
        let expected_with_fee: f64 = -(50000.0 * 0.994_f64).ln();
        assert!((weight_with_fee - expected_with_fee).abs() < 1e-10);

        // Fee should increase weight (make edge less attractive)
        assert!(weight_with_fee > weight_no_fee);
    }

    #[test]
    fn test_remove_edge() {
        let currencies = vec!["BTC".to_string(), "USD".to_string()];
        let mut matrix = AdjacencyMatrix::new(currencies);

        // Add edge
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();
        assert!(matrix.has_edge(0, 1));

        // Remove edge
        matrix.remove_edge("BTC", "USD").unwrap();
        assert!(!matrix.has_edge(0, 1));
        assert_eq!(matrix.get_weight(0, 1), f64::INFINITY);
        assert_eq!(matrix.get_rate(0, 1), Decimal::ZERO);
    }

    #[test]
    fn test_neighbors() {
        let currencies = vec!["BTC".to_string(), "ETH".to_string(), "USD".to_string()];
        let mut matrix = AdjacencyMatrix::new(currencies);

        // BTC -> USD
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();
        // BTC -> ETH
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();

        let btc_idx = matrix.get_index("BTC").unwrap();
        let neighbors = matrix.neighbors(btc_idx);

        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&matrix.get_index("USD").unwrap()));
        assert!(neighbors.contains(&matrix.get_index("ETH").unwrap()));
    }

    #[test]
    fn test_memory_size() {
        let currencies = vec!["BTC".to_string(), "USD".to_string(), "ETH".to_string()];
        let matrix = AdjacencyMatrix::new(currencies);

        let memory = matrix.memory_size();

        // For 3 currencies: 3×3 = 9 cells
        // weights: 9 × 8 bytes (f64) = 72 bytes
        // rates: 9 × 16 bytes (Decimal) = 144 bytes
        // hash: ~small
        assert!(memory > 200); // At least 216 bytes
        assert!(memory < 1000); // Should be well under 1KB
    }

    #[test]
    fn test_cache_efficiency_target() {
        // For 200 currencies (realistic Coinbase graph)
        let size = 200;
        let currencies: Vec<String> = (0..size).map(|i| format!("C{}", i)).collect();
        let matrix = AdjacencyMatrix::new(currencies);

        let memory = matrix.memory_size();

        // Target: fit in L1 cache (32KB)
        // 200×200 = 40,000 cells
        // weights: 40,000 × 8 = 320KB
        // rates: 40,000 × 16 = 640KB
        // Total: ~1MB (exceeds L1, but fits in L2)

        println!(
            "Memory for 200 currencies: {} bytes ({} KB)",
            memory,
            memory / 1024
        );

        // This will exceed L1 (32KB) but should fit in L2 (256KB-1MB)
        assert!(memory < 2_000_000); // Should be under 2MB
    }
}
