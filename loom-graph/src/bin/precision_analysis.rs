// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use loom_graph::precision_analysis;

fn main() {
    println!("Starting numerical precision analysis for Loom...\n");

    // Run comprehensive precision report
    precision_analysis::generate_precision_report();

    println!("\n✓ Analysis complete. Review results above to inform decision.");
}
