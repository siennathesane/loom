# Coinbase FIX 5.0 SP2 Compliance Test Suite

This document describes the comprehensive Coinbase FIX API compliance test suite in `coinbase_compliance_tests.rs`.

## Overview

The test suite validates compliance with Coinbase's FIX 5.0 SP2 API specification across all critical areas:

- **Connection & Authentication**: HMAC-SHA256, TLS, protocol versions
- **Session Management**: Heartbeats, sequence numbers, resend handling
- **Rate Limiting**: Message velocity, connection limits
- **Market Data**: SecurityList, MarketData flows
- **Order Entry**: NewOrderSingle, ExecutionReport, batch operations
- **Drop Copy**: Read-only session establishment
- **Error Handling**: Rejects, invalid messages, sequence gaps

## Test Categories

### 1. Connectivity Tests (Per Coinbase Connectivity Spec)

#### `test_logon_begin_string_fixt11`
- **Requirement**: BeginString(8) must be "FIXT.1.1"
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: FIX protocol version compliance

#### `test_logon_default_appl_ver_id`
- **Requirement**: DefaultApplVerID(1137) must be "9" (FIX 5.0 SP2)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Application version specification

#### `test_logon_hmac_sha256_signature`
- **Requirement**: Logon must include Username(553), Password(554), RawDataLength(95), RawData(96)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: HMAC-SHA256 authentication signature

#### `test_logon_target_comp_id_coinbase`
- **Requirement**: TargetCompID(56) must be "Coinbase"
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Target identifier

#### `test_logon_heartbeat_interval_max_300`
- **Requirement**: HeartBtInt(108) must be ≤ 300 seconds (defaults to 10s)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Heartbeat interval constraint

#### `test_logon_encrypt_method_none`
- **Requirement**: EncryptMethod(98) must be 0 (None - SSL/TLS handles encryption)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Encryption method configuration

### 2. Session Management Tests

#### `test_sequence_numbers_start_at_one`
- **Requirement**: MsgSeqNum(34) starts at 1 for Logon
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: Sequence number initialization

#### `test_sequence_numbers_no_leading_zeros`
- **Requirement**: Sequence numbers must NOT include leading zeros
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Sequence number formatting

#### `test_sending_time_fractional_seconds`
- **Requirement**: SendingTime(52) fractional seconds must use exactly 3 digits
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Timestamp format: `YYYYMMDD-HH:MM:SS.sss`

#### `test_resend_request_not_supported`
- **Requirement**: Resend requests are NOT supported; server responds with SequenceReset-GapFill
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Resend request handling (returns GapFillFlag=Y)

#### `test_heartbeat_maintains_session`
- **Requirement**: Heartbeat messages maintain session liveness
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: Session keep-alive mechanism

#### `test_test_request_echoes_test_req_id`
- **Requirement**: Heartbeat response to TestRequest must echo TestReqID(112)
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: TestRequest/Heartbeat exchange

#### `test_logout_client_initiated`
- **Requirement**: Client Logout receives server Logout response, then connection closes
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: Logout sequence

### 3. Rate Limiting Tests

#### `test_rate_limit_logons_per_second`
- **Requirement**: Maximum 2 logons per second per API key
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/rate-limits
- **Validates**: Logon rate limiting

#### `test_rate_limit_100_requests_per_second`
- **Requirement**: Maximum 100 requests per second (normal operations)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/rate-limits
- **Validates**: Message rate limiting (lazy-fill token bucket algorithm)

#### `test_disconnect_at_200_messages_per_second`
- **Requirement**: Session disconnected if messages exceed 200/second
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/rate-limits
- **Validates**: Hard rate limit enforcement

### 4. Drop Copy Session Tests

#### `test_drop_copy_flag_required`
- **Requirement**: Drop copy session requires DropCopyFlag(9406)=Y in Logon
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/drop-copy
- **Validates**: Drop copy session establishment

#### `test_drop_copy_read_only_no_order_entry`
- **Requirement**: Drop copy sessions are read-only (no order entry messages)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/drop-copy
- **Validates**: Drop copy session restrictions
- **Status**: Placeholder for future implementation

### 5. Best Practices Compliance Tests

#### `test_one_api_key_per_connection`
- **Requirement**: Maximum 1 API key per session/connection
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/best-practices
- **Validates**: Single API key usage

#### `test_max_75_connections_per_profile`
- **Requirement**: Maximum 75 connections per profile
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/best-practices
- **Validates**: Connection limit per profile
- **Status**: Placeholder for stress testing infrastructure

#### `test_max_175_connections_per_user`
- **Requirement**: Maximum 175 connections per user across all profiles
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/best-practices
- **Validates**: Global user connection limit
- **Status**: Placeholder for multi-profile testing infrastructure

### 6. Order Entry Message Tests

#### `test_new_order_single_clordid_uuid`
- **Requirement**: ClOrdID(11) must be UUID v4 format
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Client order ID format

#### `test_new_order_single_required_fields`
- **Requirement**: NewOrderSingle requires ClOrdID, Symbol(55), Side(54), OrdType(40)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Required order fields

#### `test_new_order_single_quantity_specification`
- **Requirement**: Either OrderQty(38) or CashOrderQty(152) required
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Quantity specification options

#### `test_limit_order_requires_price`
- **Requirement**: Limit orders (OrdType=2) require Price(44)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Limit order fields

#### `test_stop_limit_requires_stop_px`
- **Requirement**: Stop Limit orders (OrdType=4) require StopPx(99)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Stop limit order fields

#### `test_gtd_order_requires_expire_time`
- **Requirement**: GTD orders (TimeInForce=6) require ExpireTime(126)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Good-till-date order fields

#### `test_post_only_exec_inst`
- **Requirement**: Post-only orders use ExecInst(18)=A
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Post-only order configuration

#### `test_execution_report_required_fields`
- **Requirement**: ExecutionReport requires ClOrdID, OrderID(37), OrdStatus(39), ExecType(150), Symbol, TransactTime(60)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Execution report structure

#### `test_batch_order_max_15_orders`
- **Requirement**: Maximum 15 orders per batch message (NoOrders=73)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/best-practices
- **Validates**: Batch message size limit

#### `test_order_cancel_request_required_fields`
- **Requirement**: OrderCancelRequest requires ClOrdID, OrderID or OrigClOrdID(41), Symbol
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Cancel request structure

#### `test_order_cancel_reject_reason`
- **Requirement**: OrderCancelReject includes OrdStatus=8, CxlRejReason(102), CxlRejResponseTo(434)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Cancel reject response

### 7. Error Handling & Reject Tests

#### `test_invalid_logon_no_signature_rejected`
- **Requirement**: Logon without authentication signature should be rejected
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Authentication enforcement

#### `test_sequence_gap_triggers_resend_request`
- **Requirement**: Sequence gap triggers ResendRequest, Logout, or Reject
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: Sequence gap detection

#### `test_invalid_checksum_rejected`
- **Requirement**: Messages with invalid checksum should be rejected
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: Checksum validation

#### `test_malformed_message_handling`
- **Requirement**: Malformed messages should result in connection close or ignore
- **Reference**: FIX 5.0 SP2 standard
- **Validates**: Protocol error handling

### 8. Market Data Tests (Integration)

#### `test_security_list_request_flow`
- **Requirement**: SecurityListRequest(35=x) receives SecurityList(35=y) with echoed SecurityReqID(320)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Security list request/response flow

#### `test_market_data_request_subscription`
- **Requirement**: MarketDataRequest(35=V) receives MarketDataIncrementalRefresh(35=X) or MarketDataSnapshotFullRefresh(35=W)
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/connectivity
- **Validates**: Market data subscription flow

### 9. Integration Test: Full Order Lifecycle

#### `test_full_order_lifecycle`
- **Requirement**: NewOrderSingle → ExecutionReport flow with proper fields
- **Reference**: https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages
- **Validates**: Complete order submission and acknowledgment

### 10. Helper Function Tests

#### `test_uuid_v4_format`
- **Validates**: UUID v4 generation and format

#### `test_fix_timestamp_format`
- **Validates**: FIX timestamp format (YYYYMMDD-HH:MM:SS.sss)

## Running the Tests

### Run all compliance tests:
```bash
cargo test --test coinbase_compliance_tests
```

### Run specific test category:
```bash
# Connectivity tests
cargo test --test coinbase_compliance_tests test_logon

# Session management tests
cargo test --test coinbase_compliance_tests test_sequence

# Rate limiting tests
cargo test --test coinbase_compliance_tests test_rate_limit

# Order entry tests
cargo test --test coinbase_compliance_tests test_new_order

# Market data tests
cargo test --test coinbase_compliance_tests test_security_list
cargo test --test coinbase_compliance_tests test_market_data
```

### Run against Coinbase sandbox:
To run tests against the real Coinbase sandbox environment, update the server configuration in test helpers to point to:
- Order Entry: `tcp+ssl://fix-ord.sandbox.exchange.coinbase.com:6121`
- Market Data: `tcp+ssl://fix-md.sandbox.exchange.coinbase.com:6121`
- Drop Copy: `tcp+ssl://fix-dc.sandbox.exchange.coinbase.com:6122`

## Test Infrastructure

### Helper Functions

- `test_client_config()`: Creates standard test client configuration
- `test_server_config()`: Creates mock server configuration
- `setup_server()`: Starts mock server and returns address
- `connect_and_logon(addr)`: Establishes authenticated connection

### Mock Server

The tests use `MockCoinbaseServer` from `loom-harness` which simulates Coinbase FIX API behavior for:
- Session-level protocol (Logon, Heartbeat, TestRequest, etc.)
- Market data feeds
- Order entry messages
- Rate limiting
- Error scenarios

## Coverage Summary

The test suite covers the following Coinbase FIX API specification areas:

| Area | Coverage | Test Count |
|------|----------|------------|
| Connectivity | ✅ Complete | 6 |
| Session Management | ✅ Complete | 7 |
| Rate Limiting | ⚠️ Partial | 3 |
| Drop Copy | ⚠️ Partial | 2 |
| Best Practices | ⚠️ Documented | 3 |
| Order Entry | ✅ Complete | 11 |
| Error Handling | ✅ Complete | 4 |
| Market Data | ✅ Complete | 2 |
| Integration | ✅ Complete | 1 |
| Helpers | ✅ Complete | 2 |

**Total Tests**: 41

### Legend
- ✅ Complete: Full implementation with assertions
- ⚠️ Partial: Basic implementation, needs enhancement
- ⚠️ Documented: Test structure exists, awaiting infrastructure

## Known Limitations

1. **Rate Limiting Tests**: Full stress testing requires production-scale infrastructure
2. **Connection Limits**: Tests for 75/175 connection limits are placeholders
3. **Drop Copy**: Read-only enforcement requires mock server enhancement
4. **Resend Requests**: Tests document expected behavior (not supported)
5. **TLS/SSL**: Tests run over TCP; production requires TLS 1.2+

## Future Enhancements

1. Add stress testing infrastructure for connection limits
2. Implement multi-profile testing for user-level connection limits
3. Enhance mock server to track drop copy state
4. Add performance benchmarking for message throughput
5. Implement TLS connection testing
6. Add tests for all order types (TPSL, RFQ, etc.)
7. Expand batch operation testing (OrderCancelBatch, NewOrderBatch)
8. Add market data snapshot/incremental refresh validation
9. Test message history retrieval (4-hour window)
10. Validate fee reporting in ExecutionReport

## References

- [Coinbase FIX API Connectivity](https://docs.cdp.coinbase.com/exchange/fix-api/connectivity)
- [Coinbase FIX API Best Practices](https://docs.cdp.coinbase.com/exchange/fix-api/best-practices)
- [Coinbase FIX API Rate Limits](https://docs.cdp.coinbase.com/exchange/fix-api/rate-limits)
- [Coinbase FIX API Drop Copy](https://docs.cdp.coinbase.com/exchange/fix-api/drop-copy)
- [Coinbase FIX API Order Entry Messages](https://docs.cdp.coinbase.com/exchange/fix-api/order-entry-messages/order-entry-messages5)
- [FIX 5.0 SP2 Specification](https://www.fixtrading.org/standards/fix-5-0-sp-2/)

## Contributing

When adding new compliance tests:

1. Reference the specific Coinbase FIX API documentation page
2. Include the requirement being tested in the test docstring
3. Use descriptive test names that indicate the requirement
4. Add the test to the appropriate category in this README
5. Update the coverage summary table
