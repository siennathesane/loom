// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! HMAC-SHA256 authentication for FIX Logon.
//!
//! Implements the authentication mechanism required by Coinbase:
//! - Tag 553 (Username) carries the API key
//! - RawData (tag 93) carries the base64-encoded HMAC signature
//!
//! Signature base string format (as per Coinbase spec):
//! SendingTime + \x01 + MsgType + \x01 + MsgSeqNum + \x01 + SenderCompID + \x01 + TargetCompID + \x01 + Passphrase

use base64::{engine::general_purpose, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 signature for FIX Logon
///
/// Signature format (Coinbase specification):
/// SendingTime + \x01 + MsgType + \x01 + MsgSeqNum + \x01 + SenderCompID + \x01 + TargetCompID + \x01 + Passphrase
///
/// # Arguments
/// * `secret` - The API secret key (base64-encoded, will be decoded)
/// * `sender_comp_id` - SenderCompID field value (API key)
/// * `target_comp_id` - TargetCompID field value (usually "Coinbase")
/// * `msg_type` - MsgType field value (usually "A" for Logon)
/// * `msg_seq_num` - MsgSeqNum field value
/// * `sending_time` - SendingTime field value (FIX timestamp format)
/// * `passphrase` - API passphrase
///
/// # Returns
/// Base64-encoded HMAC signature
pub fn compute_logon_hmac(
    secret: &str,
    sender_comp_id: &str,
    target_comp_id: &str,
    msg_type: &str,
    msg_seq_num: u64,
    sending_time: &str,
    passphrase: &str,
) -> String {
    // Build the signature message per Coinbase spec with SOH (\x01) delimiters
    let message = format!(
        "{}\x01{}\x01{}\x01{}\x01{}\x01{}",
        sending_time, msg_type, msg_seq_num, sender_comp_id, target_comp_id, passphrase
    );

    // Decode the base64 secret
    let decoded_secret = general_purpose::STANDARD
        .decode(secret.as_bytes())
        .expect("API secret must be valid base64");

    // Compute HMAC using decoded secret
    let mut mac =
        HmacSha256::new_from_slice(&decoded_secret).expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();

    // Encode as base64
    general_purpose::STANDARD.encode(&code_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_logon_hmac() {
        // Test with known values (secret must be base64)
        let secret = general_purpose::STANDARD.encode("test_secret");
        let sender_comp_id = "API_KEY_123";
        let target_comp_id = "Coinbase";
        let msg_type = "A";
        let msg_seq_num = 1;
        let sending_time = "20260212-12:00:00.000";
        let passphrase = "test_passphrase";

        let hmac = compute_logon_hmac(
            &secret,
            sender_comp_id,
            target_comp_id,
            msg_type,
            msg_seq_num,
            sending_time,
            passphrase,
        );

        // The result should be a valid base64 string
        assert!(!hmac.is_empty());
        assert!(general_purpose::STANDARD.decode(&hmac).is_ok());

        // Computing the same HMAC again should give the same result (deterministic)
        let hmac2 = compute_logon_hmac(
            &secret,
            sender_comp_id,
            target_comp_id,
            msg_type,
            msg_seq_num,
            sending_time,
            passphrase,
        );
        assert_eq!(hmac, hmac2);
    }

    #[test]
    fn test_hmac_changes_with_inputs() {
        let secret = general_purpose::STANDARD.encode("test_secret");
        let sender_comp_id = "API_KEY_123";
        let target_comp_id = "Coinbase";
        let msg_type = "A";
        let msg_seq_num = 1;
        let sending_time = "20260212-12:00:00.000";
        let passphrase = "test_passphrase";

        let hmac1 = compute_logon_hmac(
            &secret,
            sender_comp_id,
            target_comp_id,
            msg_type,
            msg_seq_num,
            sending_time,
            passphrase,
        );

        // Change one parameter (sequence number)
        let hmac2 = compute_logon_hmac(
            &secret,
            sender_comp_id,
            target_comp_id,
            msg_type,
            msg_seq_num + 1, // Changed
            sending_time,
            passphrase,
        );

        assert_ne!(hmac1, hmac2);
    }

    #[test]
    fn test_hmac_signature_format() {
        // Test that signature format matches Coinbase spec
        // Format: SendingTime + \x01 + MsgType + \x01 + MsgSeqNum + \x01 + SenderCompID + \x01 + TargetCompID + \x01 + Passphrase

        let secret = general_purpose::STANDARD.encode("secret");
        let sender_comp_id = "SENDER";
        let target_comp_id = "TARGET";
        let msg_type = "A";
        let msg_seq_num = 1;
        let sending_time = "20260212-12:00:00";
        let passphrase = "pass";

        let signature = compute_logon_hmac(
            &secret,
            sender_comp_id,
            target_comp_id,
            msg_type,
            msg_seq_num,
            sending_time,
            passphrase,
        );

        // Verify it's different from wrong orderings
        let wrong_order = compute_logon_hmac(
            &secret,
            target_comp_id, // Wrong: swapped sender and target
            sender_comp_id,
            msg_type,
            msg_seq_num,
            sending_time,
            passphrase,
        );

        assert_ne!(signature, wrong_order);
    }
}
