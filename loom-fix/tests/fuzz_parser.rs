// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Fuzz testing for FIX parser
//!
//! This test generates 100K+ random byte sequences and verifies that
//! the parser handles all of them without panicking.

use loom_fix::Parser;

#[test]
fn fuzz_parser_with_random_bytes() {
    const NUM_ITERATIONS: usize = 100_000;

    let mut parser = Parser::new();
    let mut panic_count = 0;

    for i in 0..NUM_ITERATIONS {
        // Generate pseudo-random bytes using a simple PRNG
        let seed = i as u64;
        let mut bytes = Vec::with_capacity(1024);

        for j in 0..1024 {
            let byte = ((seed.wrapping_mul(1103515245).wrapping_add(12345 + j as u64)) >> 16) as u8;
            bytes.push(byte);
        }

        // Feed to parser
        parser.clear();
        parser.feed(&bytes);

        // Try to parse - should never panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = parser.parse_message();
        }));

        if result.is_err() {
            panic_count += 1;
        }
    }

    assert_eq!(
        panic_count, 0,
        "Parser panicked {} times out of {} iterations",
        panic_count, NUM_ITERATIONS
    );
}

#[test]
fn fuzz_parser_with_partial_messages() {
    const NUM_ITERATIONS: usize = 10_000;

    let mut parser = Parser::new();

    for i in 0..NUM_ITERATIONS {
        parser.clear();

        // Create a valid-looking but incomplete message
        let partial = format!("8=FIXT.1.1\x019={}\x0135=0\x01", i);
        parser.feed(partial.as_bytes());

        // Should return None (incomplete), not panic or error
        let result = parser.parse_message();

        // Either incomplete or error is acceptable, but no panics
        if let Ok(Some(_)) = result {
            // Shouldn't get a complete message from incomplete data
            panic!(
                "Parser returned complete message from incomplete data at iteration {}",
                i
            );
        }
    }
}

#[test]
fn fuzz_parser_with_malformed_fields() {
    let non_utf8 = vec![0xFF; 10000];
    let null_bytes = vec![0x00; 1000];
    let soh_bytes = vec![0x01; 1000];

    let test_cases: Vec<&[u8]> = vec![
        b"8=WRONG\x01",
        b"8=FIXT.1.1\x019=abc\x01",
        b"8=FIXT.1.1\x019=999999999999\x01",
        b"35=INVALID\x0134=1\x0149=S\x0156=T\x0152=TIME\x01",
        b"8=FIXT.1.1\x019=10\x0135=0\x0110=999\x01",
        &non_utf8,
        &null_bytes,
        &soh_bytes,
    ];

    for (idx, test_case) in test_cases.iter().enumerate() {
        let mut parser = Parser::new();
        parser.feed(test_case);

        // Should not panic, regardless of input
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = parser.parse_message();
        }));

        assert!(result.is_ok(), "Parser panicked on test case {}", idx);
    }
}

#[test]
fn fuzz_parser_with_boundary_values() {
    let mut parser = Parser::new();

    // Test with maximum message size boundary
    let huge_body = "8=FIXT.1.1\x019=10485760\x01".to_string(); // 10MB
    parser.feed(huge_body.as_bytes());
    let result = parser.parse_message();

    // Should handle large sizes gracefully (either incomplete or error)
    assert!(matches!(result, Ok(None) | Err(_)));

    // Test with zero-length body
    parser.clear();
    let empty = b"8=FIXT.1.1\x019=0\x0110=000\x01";
    parser.feed(empty);
    let _ = parser.parse_message(); // Should not panic

    // Test with sequence numbers at boundaries
    parser.clear();
    let max_seq = format!(
        "8=FIXT.1.1\x019=30\x0135=0\x0134={}\x0149=S\x0156=T\x0152=T\x0110=000\x01",
        u64::MAX
    );
    parser.feed(max_seq.as_bytes());
    let _ = parser.parse_message(); // Should not panic
}

#[test]
fn fuzz_parser_stress_test() {
    // Stress test: mix of valid and invalid messages
    const ITERATIONS: usize = 50_000;

    let valid_msg = b"8=FIXT.1.1\x019=55\x0135=0\x0134=1\x0149=SENDER\x0152=20260212-12:00:00.000\x0156=TARGET\x0110=062\x01";
    let invalid_bytes = vec![0xFF; 100];
    let invalid_patterns: Vec<&[u8]> = vec![b"GARBAGE", b"8=BAD\x01", &invalid_bytes];

    let mut parser = Parser::new();
    let mut valid_count = 0;
    let mut error_count = 0;
    let mut incomplete_count = 0;

    for i in 0..ITERATIONS {
        parser.clear();

        if i % 10 == 0 {
            // Every 10th message is valid
            parser.feed(valid_msg);
        } else {
            // Others are random invalid patterns
            let pattern = &invalid_patterns[i % invalid_patterns.len()];
            parser.feed(pattern);
        }

        match parser.parse_message() {
            Ok(Some(_)) => valid_count += 1,
            Ok(None) => incomplete_count += 1,
            Err(_) => error_count += 1,
        }
    }

    // Verify we processed all iterations without panics
    assert_eq!(valid_count + error_count + incomplete_count, ITERATIONS);

    // We should have some valid parses (roughly ITERATIONS/10)
    assert!(valid_count > 0, "No valid messages parsed");
}
