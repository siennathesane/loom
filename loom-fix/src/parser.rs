// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! FIX message parser with state machine for streaming data.
//!
//! This parser handles FIX messages from a byte stream, validating:
//! - Proper message structure (BeginString, BodyLength, body, CheckSum)
//! - Checksum correctness
//! - Required fields presence
//!
//! The parser is designed to:
//! - Handle partial messages (streaming)
//! - Gracefully handle malformed messages without panicking
//! - Be efficient with zero-copy parsing where possible

use crate::message::{FixMessage, MsgType, Tag};
use crate::serialization::calculate_checksum;
use crate::SOH;
use std::fmt;

/// Parser error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Incomplete message, need more data
    Incomplete,
    /// Invalid BeginString
    InvalidBeginString,
    /// Invalid BodyLength
    InvalidBodyLength,
    /// Invalid CheckSum
    InvalidCheckSum,
    /// Checksum mismatch
    CheckSumMismatch { expected: u8, actual: u8 },
    /// Missing required field
    MissingRequiredField(Tag),
    /// Invalid field format
    InvalidField { tag: u32, reason: String },
    /// Invalid message type
    InvalidMsgType(String),
    /// Message too large
    MessageTooLarge(usize),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Incomplete => write!(f, "Incomplete message"),
            ParseError::InvalidBeginString => write!(f, "Invalid BeginString"),
            ParseError::InvalidBodyLength => write!(f, "Invalid BodyLength"),
            ParseError::InvalidCheckSum => write!(f, "Invalid CheckSum"),
            ParseError::CheckSumMismatch { expected, actual } => {
                write!(
                    f,
                    "CheckSum mismatch: expected {}, got {}",
                    expected, actual
                )
            }
            ParseError::MissingRequiredField(tag) => {
                write!(f, "Missing required field: {}", tag)
            }
            ParseError::InvalidField { tag, reason } => {
                write!(f, "Invalid field {}: {}", tag, reason)
            }
            ParseError::InvalidMsgType(s) => write!(f, "Invalid MsgType: {}", s),
            ParseError::MessageTooLarge(size) => write!(f, "Message too large: {} bytes", size),
        }
    }
}

impl std::error::Error for ParseError {}

/// Maximum message size to prevent DoS (10MB)
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// FIX message parser
pub struct Parser {
    buffer: Vec<u8>,
}

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
        }
    }

    /// Add data to the parser's buffer
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to parse a complete message from the buffer
    ///
    /// Returns:
    /// - Ok(Some(msg)) if a complete message was parsed
    /// - Ok(None) if more data is needed
    /// - Err(e) if the message is malformed
    pub fn parse_message(&mut self) -> Result<Option<FixMessage>, ParseError> {
        // Check if we have enough data for minimal message
        if self.buffer.len() < 20 {
            return Ok(None);
        }

        // Parse BeginString (8=FIXT.1.1\x01)
        let begin_string_end = self.find_soh(0)?;
        let begin_string_field = &self.buffer[0..begin_string_end];

        if !begin_string_field.starts_with(b"8=") {
            return Err(ParseError::InvalidBeginString);
        }

        let begin_string = &begin_string_field[2..];
        if begin_string != crate::BEGIN_STRING.as_bytes() {
            return Err(ParseError::InvalidBeginString);
        }

        // Parse BodyLength (9=<length>\x01)
        let body_length_start = begin_string_end + 1;
        let body_length_end = self.find_soh(body_length_start)?;
        let body_length_field = &self.buffer[body_length_start..body_length_end];

        if !body_length_field.starts_with(b"9=") {
            return Err(ParseError::InvalidBodyLength);
        }

        let body_length_str = std::str::from_utf8(&body_length_field[2..])
            .map_err(|_| ParseError::InvalidBodyLength)?;

        let body_length: usize = body_length_str
            .parse()
            .map_err(|_| ParseError::InvalidBodyLength)?;

        if body_length > MAX_MESSAGE_SIZE {
            return Err(ParseError::MessageTooLarge(body_length));
        }

        // Check if we have the complete message
        let body_start = body_length_end + 1;
        let body_end = body_start + body_length;
        let checksum_end = body_end + 7; // "10=XXX\x01" = 7 bytes

        if self.buffer.len() < checksum_end {
            return Ok(None); // Need more data
        }

        // Extract body
        let body = &self.buffer[body_start..body_end];

        // Parse CheckSum
        let checksum_field = &self.buffer[body_end..checksum_end];
        if !checksum_field.starts_with(b"10=") || checksum_field[6] != SOH {
            return Err(ParseError::InvalidCheckSum);
        }

        let checksum_str =
            std::str::from_utf8(&checksum_field[3..6]).map_err(|_| ParseError::InvalidCheckSum)?;

        let expected_checksum: u8 = checksum_str
            .parse()
            .map_err(|_| ParseError::InvalidCheckSum)?;

        // Verify checksum over full message (BeginString + BodyLength + Body)
        let calculated_checksum = calculate_checksum(&self.buffer[0..body_end]);
        if calculated_checksum != expected_checksum {
            return Err(ParseError::CheckSumMismatch {
                expected: expected_checksum,
                actual: calculated_checksum,
            });
        }

        // Parse body fields
        let message = self.parse_body(body)?;

        // Remove parsed message from buffer
        self.buffer.drain(0..checksum_end);

        Ok(Some(message))
    }

    /// Parse the message body into a FixMessage
    fn parse_body(&self, body: &[u8]) -> Result<FixMessage, ParseError> {
        let mut fields = Vec::new();
        let mut pos = 0;

        while pos < body.len() {
            let field_end = match body[pos..].iter().position(|&b| b == SOH) {
                Some(idx) => pos + idx,
                None => return Err(ParseError::InvalidBeginString), // Body should end with SOH
            };

            let field = &body[pos..field_end];
            let eq_pos =
                field
                    .iter()
                    .position(|&b| b == b'=')
                    .ok_or_else(|| ParseError::InvalidField {
                        tag: 0,
                        reason: "Missing '=' delimiter".to_string(),
                    })?;

            let tag_str =
                std::str::from_utf8(&field[0..eq_pos]).map_err(|_| ParseError::InvalidField {
                    tag: 0,
                    reason: "Invalid UTF-8 in tag".to_string(),
                })?;

            let tag_num: u32 = tag_str.parse().map_err(|_| ParseError::InvalidField {
                tag: 0,
                reason: "Tag is not a number".to_string(),
            })?;

            let value = std::str::from_utf8(&field[eq_pos + 1..])
                .map_err(|_| ParseError::InvalidField {
                    tag: tag_num,
                    reason: "Invalid UTF-8 in value".to_string(),
                })?
                .to_string();

            fields.push((tag_num, value));
            pos = field_end + 1;
        }

        // Extract required fields
        let msg_type_str = fields
            .iter()
            .find(|(t, _)| *t == 35)
            .map(|(_, v)| v.as_str())
            .ok_or(ParseError::MissingRequiredField(Tag::MsgType))?;

        let msg_type = MsgType::from_str(msg_type_str)
            .ok_or_else(|| ParseError::InvalidMsgType(msg_type_str.to_string()))?;

        let msg_seq_num_str = fields
            .iter()
            .find(|(t, _)| *t == 34)
            .map(|(_, v)| v.as_str())
            .ok_or(ParseError::MissingRequiredField(Tag::MsgSeqNum))?;

        let msg_seq_num: u64 = msg_seq_num_str
            .parse()
            .map_err(|_| ParseError::InvalidField {
                tag: 34,
                reason: "MsgSeqNum is not a valid number".to_string(),
            })?;

        let sender_comp_id = fields
            .iter()
            .find(|(t, _)| *t == 49)
            .map(|(_, v)| v.clone())
            .ok_or(ParseError::MissingRequiredField(Tag::SenderCompID))?;

        let target_comp_id = fields
            .iter()
            .find(|(t, _)| *t == 56)
            .map(|(_, v)| v.clone())
            .ok_or(ParseError::MissingRequiredField(Tag::TargetCompID))?;

        let sending_time = fields
            .iter()
            .find(|(t, _)| *t == 52)
            .map(|(_, v)| v.clone())
            .ok_or(ParseError::MissingRequiredField(Tag::SendingTime))?;

        // Build message
        let mut message = FixMessage::new(
            msg_type,
            msg_seq_num,
            sender_comp_id,
            target_comp_id,
            sending_time,
        );

        // Add remaining fields
        for (tag_num, value) in fields {
            if let Some(tag) = Tag::from_u32(tag_num) {
                // Skip already-parsed header fields
                match tag {
                    Tag::MsgType
                    | Tag::MsgSeqNum
                    | Tag::SenderCompID
                    | Tag::TargetCompID
                    | Tag::SendingTime => continue,
                    _ => message.add_field(tag, value),
                }
            }
        }

        Ok(message)
    }

    /// Find the next SOH character starting from pos
    fn find_soh(&self, start: usize) -> Result<usize, ParseError> {
        self.buffer[start..]
            .iter()
            .position(|&b| b == SOH)
            .map(|idx| start + idx)
            .ok_or(ParseError::Incomplete)
    }

    /// Clear the internal buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get the number of bytes currently in the buffer
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::build_fix_message;

    #[test]
    fn test_parse_valid_message() {
        let msg = FixMessage::new(
            MsgType::Heartbeat,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let serialized = build_fix_message(&msg);

        let mut parser = Parser::new();
        parser.feed(&serialized);

        let parsed = parser.parse_message().unwrap().unwrap();
        assert_eq!(parsed.msg_type, MsgType::Heartbeat);
        assert_eq!(parsed.msg_seq_num, 1);
        assert_eq!(parsed.sender_comp_id, "SENDER");
        assert_eq!(parsed.target_comp_id, "TARGET");
    }

    #[test]
    fn test_parse_incomplete_message() {
        let msg = FixMessage::new(
            MsgType::Heartbeat,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let serialized = build_fix_message(&msg);

        // Feed only half the message
        let mut parser = Parser::new();
        parser.feed(&serialized[..serialized.len() / 2]);

        let result = parser.parse_message().unwrap();
        assert!(result.is_none()); // Should indicate incomplete
    }

    #[test]
    fn test_parse_checksum_mismatch() {
        let msg = FixMessage::new(
            MsgType::Heartbeat,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let mut serialized = build_fix_message(&msg);

        // Find the checksum field and corrupt the first digit
        let msg_str = String::from_utf8_lossy(&serialized);
        let checksum_marker = "10=";
        let checksum_pos = msg_str.find(checksum_marker).unwrap() + checksum_marker.len();

        // Corrupt the first checksum digit
        serialized[checksum_pos] = if serialized[checksum_pos] == b'0' {
            b'1'
        } else {
            b'0'
        };

        let mut parser = Parser::new();
        parser.feed(&serialized);

        let result = parser.parse_message();
        assert!(matches!(result, Err(ParseError::CheckSumMismatch { .. })));
    }

    #[test]
    fn test_parse_with_additional_fields() {
        let mut msg = FixMessage::new(
            MsgType::Logon,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::HeartBtInt, "30".to_string());
        msg.add_field(Tag::EncryptMethod, "0".to_string());

        let serialized = build_fix_message(&msg);

        let mut parser = Parser::new();
        parser.feed(&serialized);

        let parsed = parser.parse_message().unwrap().unwrap();
        assert_eq!(parsed.msg_type, MsgType::Logon);
        assert_eq!(parsed.get_field(Tag::HeartBtInt), Some("30"));
        assert_eq!(parsed.get_field(Tag::EncryptMethod), Some("0"));
    }

    #[test]
    fn test_parse_multiple_messages() {
        let msg1 = FixMessage::new(
            MsgType::Heartbeat,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        let msg2 = FixMessage::new(
            MsgType::Heartbeat,
            2,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:01.000".to_string(),
        );

        let serialized1 = build_fix_message(&msg1);
        let serialized2 = build_fix_message(&msg2);

        let mut combined = serialized1;
        combined.extend_from_slice(&serialized2);

        let mut parser = Parser::new();
        parser.feed(&combined);

        let parsed1 = parser.parse_message().unwrap().unwrap();
        assert_eq!(parsed1.msg_seq_num, 1);

        let parsed2 = parser.parse_message().unwrap().unwrap();
        assert_eq!(parsed2.msg_seq_num, 2);

        let parsed3 = parser.parse_message().unwrap();
        assert!(parsed3.is_none());
    }

    #[test]
    fn test_parser_handles_malformed_without_panic() {
        let malformed_inputs = vec![
            b"8=INVALID\x01".to_vec(),
            b"8=FIXT.1.1\x019=NOTANUMBER\x01".to_vec(),
            b"GARBAGE DATA".to_vec(),
            vec![0xFF; 100], // Non-UTF8 bytes
        ];

        for input in malformed_inputs {
            let mut parser = Parser::new();
            parser.feed(&input);
            let _result = parser.parse_message(); // Should not panic
        }
    }
}
