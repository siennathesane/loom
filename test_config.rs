// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use loom_config::LoomConfig;

fn main() {
    match LoomConfig::from_file("loom.toml") {
        Ok(config) => {
            println!("✅ Configuration loaded successfully!");
            println!("   FIX endpoint: {}", config.fix.address);
            println!("   Server name: {}", config.fix.server_name);
            println!("   API key: {}...", &config.fix.sender_comp_id[..8]);
            println!("   Transport: {}", config.server.transport);
            println!("   Socket: {}", config.server.address);
            println!("   Database: {}", config.database.path);
        }
        Err(e) => {
            eprintln!("❌ Failed to load config: {}", e);
            std::process::exit(1);
        }
    }
}
