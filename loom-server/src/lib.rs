// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-server
//!
//! gRPC service implementation that exposes loom-engine state to clients.
//!
//! Wraps the core arbitrage engine and provides query endpoints for loomctl:
//! status queries, pair listings, opportunity discovery, and event streaming.
//! Implements the `LoomControl` service with configurable transport (Unix socket or TCP).
//!
//! ## Examples
//!
//! ```ignore
//! let service = LoomControlService::new(db_path);
//! let server = Server::builder()
//!     .add_service(LoomControlServer::new(service))
//!     .serve(addr)
//!     .await?;
//! ```

use loom_proto::{
    loom_control_server::{LoomControl, LoomControlServer},
    *,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

#[cfg(test)]
use tokio_stream::StreamExt;

pub struct LoomControlService {
    engine_state: Arc<Mutex<EngineState>>,
    event_tx: tokio::sync::broadcast::Sender<Event>,
    db_path: String,
}

struct EngineState {
    mode: String,
    connected: bool,
    start_time: Instant,
    opportunities_detected: u64,
    ticks_processed: u64,
    graph_size: (u32, u32), // (nodes, edges)
    currencies: Vec<String>,
}

impl LoomControlService {
    pub fn new(db_path: String) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(1000);

        Self {
            engine_state: Arc::new(Mutex::new(EngineState {
                mode: "LISTEN".to_string(),
                connected: false,
                start_time: Instant::now(),
                opportunities_detected: 0,
                ticks_processed: 0,
                graph_size: (0, 0),
                currencies: Vec::new(),
            })),
            event_tx,
            db_path,
        }
    }

    pub fn update_state(
        &self,
        connected: bool,
        opportunities: u64,
        ticks: u64,
        graph_size: (u32, u32),
    ) {
        let mut state = self.engine_state.lock().unwrap();
        state.connected = connected;
        state.opportunities_detected = opportunities;
        state.ticks_processed = ticks;
        state.graph_size = graph_size;
    }

    pub fn update_currencies(&self, currencies: Vec<String>) {
        let mut state = self.engine_state.lock().unwrap();
        state.currencies = currencies;
    }

    pub fn broadcast_event(&self, event_type: &str, data: &str) {
        let event = Event {
            event_type: event_type.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            data: data.to_string(),
        };
        let _ = self.event_tx.send(event);
    }
}

#[tonic::async_trait]
impl LoomControl for LoomControlService {
    async fn get_status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let state = self.engine_state.lock().unwrap();
        let uptime = state.start_time.elapsed().as_secs();

        Ok(Response::new(StatusResponse {
            mode: state.mode.clone(),
            connected: state.connected,
            uptime_secs: uptime,
            opportunities_detected: state.opportunities_detected,
            ticks_processed: state.ticks_processed,
        }))
    }

    async fn shutdown(
        &self,
        _request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        Ok(Response::new(ShutdownResponse { success: true }))
    }

    async fn get_mode(
        &self,
        _request: Request<GetModeRequest>,
    ) -> Result<Response<GetModeResponse>, Status> {
        let state = self.engine_state.lock().unwrap();
        Ok(Response::new(GetModeResponse {
            mode: state.mode.clone(),
        }))
    }

    async fn set_mode(
        &self,
        _request: Request<SetModeRequest>,
    ) -> Result<Response<SetModeResponse>, Status> {
        Ok(Response::new(SetModeResponse {
            success: false,
            error: "Not implemented yet".to_string(),
        }))
    }

    async fn list_lump_sums(
        &self,
        _request: Request<ListLumpSumsRequest>,
    ) -> Result<Response<ListLumpSumsResponse>, Status> {
        Ok(Response::new(ListLumpSumsResponse { lump_sums: vec![] }))
    }

    async fn get_lump_sum(
        &self,
        _request: Request<GetLumpSumRequest>,
    ) -> Result<Response<GetLumpSumResponse>, Status> {
        Err(Status::not_found("Lump sum not found"))
    }

    async fn list_opportunities(
        &self,
        request: Request<ListOpportunitiesRequest>,
    ) -> Result<Response<ListOpportunitiesResponse>, Status> {
        let req = request.get_ref();
        let limit = req.limit.max(1).min(1000);

        // Build dynamic query with filters
        let mut query = String::from(
            "SELECT opportunity_id, detected_at, cycle_json, cycle_symbols,
                    expected_profit, max_return_cost, gated
             FROM opportunities WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Time range filters
        if req.start_time > 0 {
            query.push_str(" AND detected_at >= ?");
            params.push(Box::new(req.start_time));
        }
        if req.end_time > 0 {
            query.push_str(" AND detected_at <= ?");
            params.push(Box::new(req.end_time));
        }

        // Gated filter
        if req.gated_only {
            query.push_str(" AND gated = 1");
        } else if req.ungated_only {
            query.push_str(" AND gated = 0");
        }

        query.push_str(" ORDER BY detected_at DESC LIMIT ?");
        params.push(Box::new(limit));

        // Query SQLite
        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| Status::internal(format!("Query error: {}", e)))?;

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let opportunities: Vec<OpportunityInfo> = stmt
            .query_map(&param_refs[..], |row| {
                Ok(OpportunityInfo {
                    id: row.get(0)?,
                    cycle: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                    expected_profit: row.get(4)?,
                    max_return_cost: row.get(5)?,
                    gated: row.get::<_, i32>(6)? != 0,
                    detected_at: row.get(1)?,
                })
            })
            .map_err(|e| Status::internal(format!("Query error: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(Response::new(ListOpportunitiesResponse { opportunities }))
    }

    async fn get_graph_state(
        &self,
        _request: Request<GraphStateRequest>,
    ) -> Result<Response<GraphStateResponse>, Status> {
        let state = self.engine_state.lock().unwrap();
        let (nodes, edges) = state.graph_size;

        Ok(Response::new(GraphStateResponse {
            node_count: nodes,
            edge_count: edges,
            currencies: state.currencies.clone(),
        }))
    }

    async fn get_node_info(
        &self,
        _request: Request<NodeInfoRequest>,
    ) -> Result<Response<NodeInfoResponse>, Status> {
        Err(Status::not_found("Node not found"))
    }

    type StreamEventsStream = ReceiverStream<Result<Event, Status>>;

    async fn stream_events(
        &self,
        request: Request<EventStreamRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let filter = request.get_ref().event_types.clone();
        let mut event_rx = self.event_tx.subscribe();
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                // Apply filter
                if filter.is_empty() || filter.contains(&event.event_type) {
                    if tx.send(Ok(event)).await.is_err() {
                        break; // Client disconnected
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn export_ticks(
        &self,
        request: Request<ExportTicksRequest>,
    ) -> Result<Response<ExportTicksResponse>, Status> {
        let req = request.get_ref();

        // Build dynamic query with filters
        let mut query = String::from(
            "SELECT timestamp, instrument, side, price, quantity
             FROM ticks WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Time range filters
        if req.start_time > 0 {
            query.push_str(" AND timestamp >= ?");
            params.push(Box::new(req.start_time));
        }
        if req.end_time > 0 {
            query.push_str(" AND timestamp <= ?");
            params.push(Box::new(req.end_time));
        }

        // Instrument filter
        if !req.instrument.is_empty() {
            query.push_str(" AND instrument = ?");
            params.push(Box::new(req.instrument.clone()));
        }

        query.push_str(" ORDER BY timestamp ASC");

        // Query SQLite
        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| Status::internal(format!("Query error: {}", e)))?;

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let ticks: Vec<TickData> = stmt
            .query_map(&param_refs[..], |row| {
                Ok(TickData {
                    timestamp: row.get(0)?,
                    instrument: row.get(1)?,
                    side: row.get(2)?,
                    price: row.get(3)?,
                    quantity: row.get(4)?,
                })
            })
            .map_err(|e| Status::internal(format!("Query error: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(Response::new(ExportTicksResponse { ticks }))
    }
}

pub enum TransportType {
    Tcp(String),
    Unix(String), // Path to socket file
}

pub async fn start_server(
    transport: TransportType,
    db_path: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = LoomControlService::new(db_path);
    let server = LoomControlServer::new(service);

    match transport {
        TransportType::Tcp(addr) => {
            Server::builder()
                .add_service(server)
                .serve(addr.parse()?)
                .await?;
        }
        TransportType::Unix(socket_path) => {
            // Remove old socket if exists
            let _ = std::fs::remove_file(&socket_path);

            let uds = tokio::net::UnixListener::bind(&socket_path)?;
            let uds_stream = tokio_stream::wrappers::UnixListenerStream::new(uds);

            Server::builder()
                .add_service(server)
                .serve_with_incoming(uds_stream)
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Helper function to create temp database
    fn create_test_db() -> String {
        let path = format!(
            "/tmp/test_loom_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        loom_store::initialize_database(&path).unwrap();
        path
    }

    // Helper function to cleanup test database
    fn cleanup_test_db(path: &str) {
        let _ = fs::remove_file(path);
    }

    // ========== State Management Tests ==========

    #[tokio::test]
    async fn test_state_initialization() {
        let service = LoomControlService::new(":memory:".to_string());
        let state = service.engine_state.lock().unwrap();

        assert_eq!(state.mode, "LISTEN");
        assert_eq!(state.connected, false);
        assert_eq!(state.opportunities_detected, 0);
        assert_eq!(state.ticks_processed, 0);
        assert_eq!(state.graph_size, (0, 0));
    }

    #[tokio::test]
    async fn test_state_updates_reflected_in_status() {
        let service = LoomControlService::new(":memory:".to_string());

        // Update state
        service.update_state(true, 42, 1000, (10, 50));

        // Query via RPC
        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();

        assert_eq!(response.get_ref().connected, true);
        assert_eq!(response.get_ref().opportunities_detected, 42);
        assert_eq!(response.get_ref().ticks_processed, 1000);
    }

    #[tokio::test]
    async fn test_concurrent_state_updates() {
        let service = std::sync::Arc::new(LoomControlService::new(":memory:".to_string()));

        let mut handles = vec![];

        // Spawn 10 concurrent updates
        for i in 0..10u32 {
            let service_clone = service.clone();
            let handle = tokio::spawn(async move {
                service_clone.update_state(true, i as u64 * 10, i as u64 * 100, (i, i * 2));
            });
            handles.push(handle);
        }

        // Wait for all updates
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify state is consistent (last update wins)
        let state = service.engine_state.lock().unwrap();
        assert!(state.opportunities_detected < 100);
        assert!(state.ticks_processed < 1000);
    }

    #[tokio::test]
    async fn test_uptime_calculation() {
        let service = LoomControlService::new(":memory:".to_string());

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();

        // Uptime should be at least 1 second after waiting
        assert!(response.get_ref().uptime_secs >= 1);
    }

    #[tokio::test]
    async fn test_graph_state_updates() {
        let service = LoomControlService::new(":memory:".to_string());

        // Update graph size
        service.update_state(false, 0, 0, (15, 75));

        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();

        assert_eq!(response.get_ref().node_count, 15);
        assert_eq!(response.get_ref().edge_count, 75);
    }

    #[tokio::test]
    async fn test_server_compiles() {
        let _service = LoomControlService::new(":memory:".to_string());
        // Basic compilation test
    }

    #[tokio::test]
    async fn test_get_status_rpc() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();

        assert_eq!(response.get_ref().mode, "LISTEN");
        assert_eq!(response.get_ref().connected, false);
    }

    #[tokio::test]
    async fn test_get_mode_rpc() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(GetModeRequest {});
        let response = service.get_mode(request).await.unwrap();

        assert_eq!(response.get_ref().mode, "LISTEN");
    }

    #[tokio::test]
    async fn test_list_lump_sums_empty() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(ListLumpSumsRequest {});
        let response = service.list_lump_sums(request).await.unwrap();

        assert_eq!(response.get_ref().lump_sums.len(), 0);
    }

    // ========== SQLite Integration Tests ==========

    #[tokio::test]
    async fn test_list_opportunities_empty_db() {
        let db_path = create_test_db();

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 0);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_list_opportunities_with_data() {
        let db_path = create_test_db();

        // Insert test opportunity
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "opp-1",
                1000u64,
                r#"["USD","EUR","GBP"]"#,
                "USD,EUR,GBP",
                0.05,
                10.0,
                0
            ],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 1);
        let opp = &response.get_ref().opportunities[0];
        assert_eq!(opp.id, "opp-1");
        assert_eq!(opp.cycle.len(), 3);
        assert!((opp.expected_profit - 0.05).abs() < 0.001);
        assert!((opp.max_return_cost - 10.0).abs() < 0.001);
        assert_eq!(opp.gated, false);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_list_opportunities_limit_enforced() {
        let db_path = create_test_db();

        // Insert 20 opportunities
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        for i in 0..20 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    format!("opp-{}", i),
                    1000u64 + i,
                    r#"["USD","EUR"]"#,
                    "USD,EUR",
                    0.01,
                    5.0,
                    0
                ],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Test limit = 5
        let request = Request::new(ListOpportunitiesRequest {
            limit: 5,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();
        assert_eq!(response.get_ref().opportunities.len(), 5);

        // Test limit = 15
        let request = Request::new(ListOpportunitiesRequest {
            limit: 15,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();
        assert_eq!(response.get_ref().opportunities.len(), 15);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_list_opportunities_order_by_time() {
        let db_path = create_test_db();

        // Insert opportunities out of order
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let timestamps = vec![3000u64, 1000u64, 2000u64];
        for (idx, ts) in timestamps.iter().enumerate() {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    format!("opp-{}", idx),
                    ts,
                    r#"["USD"]"#,
                    "USD",
                    0.01,
                    5.0,
                    0
                ],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Should be ordered DESC by detected_at (newest first)
        assert_eq!(response.get_ref().opportunities[0].id, "opp-0"); // ts=3000
        assert_eq!(response.get_ref().opportunities[1].id, "opp-2"); // ts=2000
        assert_eq!(response.get_ref().opportunities[2].id, "opp-1"); // ts=1000

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_list_opportunities_database_error_handling() {
        let service = LoomControlService::new("/nonexistent/path/db.sqlite".to_string());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let result = service.list_opportunities(request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
        assert!(err.message().contains("Database error"));
    }

    #[tokio::test]
    async fn test_gated_flag_conversion() {
        let db_path = create_test_db();

        // Insert gated and ungated opportunities
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('gated', 1000, '[]', '', 0.01, 5.0, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('ungated', 2000, '[]', '', 0.01, 5.0, 0)",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        let gated_opp = response
            .get_ref()
            .opportunities
            .iter()
            .find(|o| o.id == "gated")
            .unwrap();
        let ungated_opp = response
            .get_ref()
            .opportunities
            .iter()
            .find(|o| o.id == "ungated")
            .unwrap();

        assert_eq!(gated_opp.gated, true);
        assert_eq!(ungated_opp.gated, false);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_list_opportunities_malformed_json() {
        let db_path = create_test_db();

        // Insert opportunity with invalid JSON
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('bad-json', 1000, 'not-valid-json', '', 0.01, 5.0, 0)",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Should handle gracefully with empty array
        assert_eq!(response.get_ref().opportunities.len(), 1);
        assert_eq!(response.get_ref().opportunities[0].cycle.len(), 0);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_get_graph_state() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();

        assert_eq!(response.get_ref().node_count, 0);
        assert_eq!(response.get_ref().edge_count, 0);
        assert_eq!(response.get_ref().currencies.len(), 0);
    }

    #[tokio::test]
    async fn test_shutdown_rpc() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(ShutdownRequest {});
        let response = service.shutdown(request).await.unwrap();

        assert_eq!(response.get_ref().success, true);
    }

    #[tokio::test]
    async fn test_set_mode_not_implemented() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(SetModeRequest {
            mode: "ACTIVE".to_string(),
        });
        let response = service.set_mode(request).await.unwrap();

        assert_eq!(response.get_ref().success, false);
        assert!(response.get_ref().error.contains("Not implemented"));
    }

    #[tokio::test]
    async fn test_get_lump_sum_not_found() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(GetLumpSumRequest {
            id: "test-id".to_string(),
        });
        let result = service.get_lump_sum(request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_get_node_info_not_found() {
        let service = LoomControlService::new(":memory:".to_string());
        let request = Request::new(NodeInfoRequest {
            currency: "USD".to_string(),
        });
        let result = service.get_node_info(request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    // ========== RPC Integration Tests ==========

    #[tokio::test]
    async fn test_end_to_end_status_rpc() {
        let service = LoomControlService::new(":memory:".to_string());

        // Set initial state
        service.update_state(true, 5, 100, (3, 8));

        // Query status
        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();

        assert_eq!(response.get_ref().mode, "LISTEN");
        assert_eq!(response.get_ref().connected, true);
        assert_eq!(response.get_ref().opportunities_detected, 5);
        assert_eq!(response.get_ref().ticks_processed, 100);
    }

    #[tokio::test]
    async fn test_end_to_end_opportunity_query() {
        let db_path = create_test_db();

        // Insert test data
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('e2e-opp', 5000, '[\"BTC\",\"ETH\"]', 'BTC,ETH', 0.02, 15.0, 1)",
            [],
        )
        .unwrap();

        // Query via RPC
        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 1,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 1);
        let opp = &response.get_ref().opportunities[0];
        assert_eq!(opp.id, "e2e-opp");
        assert_eq!(opp.cycle, vec!["BTC", "ETH"]);
        assert_eq!(opp.gated, true);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_end_to_end_event_stream() {
        let service = LoomControlService::new(":memory:".to_string());

        // Start streaming
        let request = Request::new(EventStreamRequest {
            event_types: vec![],
        });
        let mut stream = service.stream_events(request).await.unwrap().into_inner();

        // Broadcast event
        service.broadcast_event("E2E_TEST", "end to end data");

        // Receive via stream
        let event = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert_eq!(event.event_type, "E2E_TEST");
        assert_eq!(event.data, "end to end data");
    }

    #[tokio::test]
    async fn test_status_rpc_with_mode_changes() {
        let service = LoomControlService::new(":memory:".to_string());

        // Initial mode
        {
            let mut state = service.engine_state.lock().unwrap();
            state.mode = "ACTIVE".to_string();
        }

        let request = Request::new(GetModeRequest {});
        let response = service.get_mode(request).await.unwrap();
        assert_eq!(response.get_ref().mode, "ACTIVE");
    }

    #[tokio::test]
    async fn test_graph_state_zero_and_nonzero() {
        let service = LoomControlService::new(":memory:".to_string());

        // Test with zero graph
        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        assert_eq!(response.get_ref().node_count, 0);
        assert_eq!(response.get_ref().edge_count, 0);

        // Update to non-zero
        service.update_state(false, 0, 0, (100, 500));

        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        assert_eq!(response.get_ref().node_count, 100);
        assert_eq!(response.get_ref().edge_count, 500);
    }

    #[tokio::test]
    async fn test_graph_state_currencies() {
        let service = LoomControlService::new(":memory:".to_string());

        // Initially empty
        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        assert_eq!(response.get_ref().currencies.len(), 0);

        // Update currencies
        service.update_currencies(vec![
            "USD".to_string(),
            "BTC".to_string(),
            "ETH".to_string(),
        ]);

        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        assert_eq!(response.get_ref().currencies.len(), 3);
        assert_eq!(response.get_ref().currencies[0], "USD");
        assert_eq!(response.get_ref().currencies[1], "BTC");
        assert_eq!(response.get_ref().currencies[2], "ETH");
    }

    #[tokio::test]
    async fn test_graph_state_currencies_update() {
        let service = LoomControlService::new(":memory:".to_string());

        // Set initial currencies
        service.update_currencies(vec!["USD".to_string(), "BTC".to_string()]);

        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        assert_eq!(response.get_ref().currencies, vec!["USD", "BTC"]);

        // Update to different currencies
        service.update_currencies(vec![
            "EUR".to_string(),
            "GBP".to_string(),
            "JPY".to_string(),
        ]);

        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        assert_eq!(response.get_ref().currencies, vec!["EUR", "GBP", "JPY"]);
    }

    // ========== Error Handling Tests ==========

    #[tokio::test]
    async fn test_invalid_database_path() {
        let service = LoomControlService::new("/this/path/does/not/exist/db.sqlite".to_string());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let result = service.list_opportunities(request).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Internal);
    }

    #[tokio::test]
    async fn test_opportunities_limit_bounds() {
        let db_path = create_test_db();
        let service = LoomControlService::new(db_path.clone());

        // Test limit = 0 (should be clamped to 1)
        let request = Request::new(ListOpportunitiesRequest {
            limit: 0,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();
        assert_eq!(response.get_ref().opportunities.len(), 0);

        // Test limit > 1000 (should be clamped to 1000)
        let request = Request::new(ListOpportunitiesRequest {
            limit: 5000,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();
        assert_eq!(response.get_ref().opportunities.len(), 0);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_concurrent_rpc_calls() {
        let service = std::sync::Arc::new(LoomControlService::new(":memory:".to_string()));

        let mut handles = vec![];

        // Spawn 50 concurrent RPC calls
        for i in 0..50 {
            let service_clone = service.clone();
            let handle = tokio::spawn(async move {
                if i % 2 == 0 {
                    let request = Request::new(StatusRequest {});
                    service_clone.get_status(request).await.unwrap();
                } else {
                    let request = Request::new(GetModeRequest {});
                    service_clone.get_mode(request).await.unwrap();
                }
            });
            handles.push(handle);
        }

        // All should complete successfully
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_event_channel_overflow_handling() {
        let service = LoomControlService::new(":memory:".to_string());
        let _rx = service.event_tx.subscribe(); // Subscribe but don't consume

        // Broadcast more than channel capacity (1000)
        for i in 0..1500 {
            service.broadcast_event("OVERFLOW_TEST", &format!("event {}", i));
        }

        // Should not panic or crash
        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await;
        assert!(response.is_ok());
    }

    // ========== Data Extraction / JSON Serialization Tests ==========

    #[tokio::test]
    async fn test_status_json_serialization() {
        let service = LoomControlService::new(":memory:".to_string());
        service.update_state(true, 42, 1000, (10, 50));

        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();
        let status = response.into_inner();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&status).unwrap();

        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Verify all fields present (using snake_case field names)
        assert_eq!(parsed["mode"], "LISTEN");
        assert_eq!(parsed["connected"], true);
        assert_eq!(parsed["opportunities_detected"], 42);
        assert_eq!(parsed["ticks_processed"], 1000);
        assert!(parsed["uptime_secs"].is_number());
    }

    #[tokio::test]
    async fn test_opportunities_json_serialization() {
        let db_path = create_test_db();

        // Insert comprehensive test data
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('json-test', 123456789, '[\"BTC\",\"ETH\",\"USDT\"]', 'BTC,ETH,USDT', 0.0123, 45.67, 1)",
            [],
        ).unwrap();

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();
        let opportunities = response.into_inner();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&opportunities).unwrap();

        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Verify structure
        assert!(parsed["opportunities"].is_array());
        let opp = &parsed["opportunities"][0];

        // Verify all fields are present and correct types (using snake_case)
        assert_eq!(opp["id"], "json-test");
        assert!(opp["cycle"].is_array());
        assert_eq!(opp["cycle"][0], "BTC");
        assert_eq!(opp["cycle"][1], "ETH");
        assert_eq!(opp["cycle"][2], "USDT");
        assert!(opp["expected_profit"].is_number());
        assert!(opp["max_return_cost"].is_number());
        assert_eq!(opp["gated"], true);
        assert_eq!(opp["detected_at"], 123456789u64);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_graph_state_json_serialization() {
        let service = LoomControlService::new(":memory:".to_string());
        service.update_state(false, 0, 0, (25, 150));

        let request = Request::new(GraphStateRequest {});
        let response = service.get_graph_state(request).await.unwrap();
        let graph_state = response.into_inner();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&graph_state).unwrap();

        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["node_count"], 25);
        assert_eq!(parsed["edge_count"], 150);
        assert!(parsed["currencies"].is_array());
    }

    #[tokio::test]
    async fn test_json_output_parseable_by_downstream_tools() {
        let db_path = create_test_db();

        // Insert multiple opportunities
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    format!("opp-{}", i),
                    1000u64 + i,
                    r#"["USD","EUR","GBP"]"#,
                    "USD,EUR,GBP",
                    0.01 * (i as f64 + 1.0),
                    10.0 + (i as f64),
                    i % 2
                ],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Serialize
        let json = serde_json::to_string_pretty(&response.into_inner()).unwrap();

        // Parse with serde_json (simulating downstream tool)
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Verify we can extract data for testing
        let opportunities = parsed["opportunities"].as_array().unwrap();
        assert_eq!(opportunities.len(), 5);

        // Verify we can iterate and extract fields (using snake_case)
        for opp in opportunities.iter() {
            assert!(opp["id"].as_str().unwrap().starts_with("opp-"));
            assert!(opp["expected_profit"].as_f64().unwrap() > 0.0);
            assert!(opp["detected_at"].as_u64().unwrap() >= 1000);

            // Verify cycle is accessible
            let cycle = opp["cycle"].as_array().unwrap();
            assert_eq!(cycle.len(), 3);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_data_roundtrip_preservation() {
        let db_path = create_test_db();

        // Insert data with specific values
        let original_cycle = vec!["BTC", "ETH", "USDC", "BTC"];
        let original_profit = 0.05432;
        let original_cost = 123.456;
        let original_timestamp = 9876543210u64;

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('roundtrip', ?1, ?2, '', ?3, ?4, 0)",
            rusqlite::params![
                original_timestamp,
                serde_json::to_string(&original_cycle).unwrap(),
                original_profit,
                original_cost
            ],
        )
        .unwrap();

        // Extract via RPC
        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 1,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Serialize to JSON
        let json = serde_json::to_string(&response.into_inner()).unwrap();

        // Parse back
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let opp = &parsed["opportunities"][0];

        // Verify data preservation (using snake_case)
        let extracted_cycle: Vec<String> = serde_json::from_value(opp["cycle"].clone()).unwrap();
        assert_eq!(extracted_cycle, original_cycle);

        let extracted_profit = opp["expected_profit"].as_f64().unwrap();
        assert!((extracted_profit - original_profit).abs() < 0.0001);

        let extracted_cost = opp["max_return_cost"].as_f64().unwrap();
        assert!((extracted_cost - original_cost).abs() < 0.001);

        let extracted_timestamp = opp["detected_at"].as_u64().unwrap();
        assert_eq!(extracted_timestamp, original_timestamp);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_empty_data_json_serialization() {
        let service = LoomControlService::new(":memory:".to_string());

        // Query status with default values
        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();
        let json = serde_json::to_string(&response.into_inner()).unwrap();

        // Should serialize successfully even with zeros
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["connected"], false);
        assert_eq!(parsed["opportunities_detected"], 0);
    }

    #[tokio::test]
    async fn test_large_dataset_json_serialization() {
        let db_path = create_test_db();

        // Insert 100 opportunities
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        for i in 0..100 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?1, ?2, ?3, '', ?4, ?5, ?6)",
                rusqlite::params![
                    format!("bulk-{}", i),
                    i as u64,
                    r#"["A","B","C","D","E"]"#,
                    (i as f64) * 0.001,
                    (i as f64) * 10.0,
                    i % 2 // Only 0 or 1 for gated
                ],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Should serialize large dataset
        let json = serde_json::to_string(&response.into_inner()).unwrap();

        // Verify JSON is valid and complete
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["opportunities"].as_array().unwrap().len(), 100);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_special_characters_in_json() {
        let db_path = create_test_db();

        // Insert opportunity with special characters in cycle
        let cycle_with_special = vec!["BTC/USD", "ETH-EUR", "USDT_GBP"];

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('special', 1000, ?1, '', 0.01, 10.0, 0)",
            rusqlite::params![serde_json::to_string(&cycle_with_special).unwrap()],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());
        let request = Request::new(ListOpportunitiesRequest {
            limit: 1,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Serialize and parse
        let json = serde_json::to_string(&response.into_inner()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let cycle: Vec<String> =
            serde_json::from_value(parsed["opportunities"][0]["cycle"].clone()).unwrap();

        assert_eq!(cycle, cycle_with_special);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_update_state() {
        let service = LoomControlService::new(":memory:".to_string());
        service.update_state(true, 42, 1000, (10, 50));

        let request = Request::new(StatusRequest {});
        let response = service.get_status(request).await.unwrap();

        assert_eq!(response.get_ref().connected, true);
        assert_eq!(response.get_ref().opportunities_detected, 42);
        assert_eq!(response.get_ref().ticks_processed, 1000);
    }

    // ========== Event Broadcasting Tests ==========

    #[tokio::test]
    async fn test_event_broadcast_single_subscriber() {
        let service = LoomControlService::new(":memory:".to_string());
        let mut rx = service.event_tx.subscribe();

        service.broadcast_event("TEST", "test data");

        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type, "TEST");
        assert_eq!(event.data, "test data");
        assert!(event.timestamp > 0);
    }

    #[tokio::test]
    async fn test_event_broadcast_multiple_subscribers() {
        let service = std::sync::Arc::new(LoomControlService::new(":memory:".to_string()));
        let mut rx1 = service.event_tx.subscribe();
        let mut rx2 = service.event_tx.subscribe();
        let mut rx3 = service.event_tx.subscribe();

        service.broadcast_event("MULTI_TEST", "broadcast data");

        // All subscribers should receive the event
        let event1 = rx1.recv().await.unwrap();
        let event2 = rx2.recv().await.unwrap();
        let event3 = rx3.recv().await.unwrap();

        assert_eq!(event1.event_type, "MULTI_TEST");
        assert_eq!(event2.event_type, "MULTI_TEST");
        assert_eq!(event3.event_type, "MULTI_TEST");
        assert_eq!(event1.data, "broadcast data");
        assert_eq!(event2.data, "broadcast data");
        assert_eq!(event3.data, "broadcast data");
    }

    #[tokio::test]
    async fn test_event_filtering_by_type() {
        let service = LoomControlService::new(":memory:".to_string());

        // Create stream with filter
        let request = Request::new(EventStreamRequest {
            event_types: vec!["OPPORTUNITY".to_string()],
        });
        let mut stream = service.stream_events(request).await.unwrap().into_inner();

        // Broadcast different event types
        service.broadcast_event("OPPORTUNITY", "opp data");
        service.broadcast_event("TICK", "tick data");
        service.broadcast_event("OPPORTUNITY", "opp data 2");

        // Should only receive OPPORTUNITY events
        let event1 = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event1.event_type, "OPPORTUNITY");
        assert_eq!(event1.data, "opp data");

        let event2 = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event2.event_type, "OPPORTUNITY");
        assert_eq!(event2.data, "opp data 2");
    }

    #[tokio::test]
    async fn test_event_stream_empty_filter() {
        let service = LoomControlService::new(":memory:".to_string());

        // Create stream with no filter (receive all)
        let request = Request::new(EventStreamRequest {
            event_types: vec![],
        });
        let mut stream = service.stream_events(request).await.unwrap().into_inner();

        // Broadcast different event types
        service.broadcast_event("TYPE_A", "data a");
        service.broadcast_event("TYPE_B", "data b");

        // Should receive all events
        let event1 = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event1.event_type, "TYPE_A");

        let event2 = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event2.event_type, "TYPE_B");
    }

    #[tokio::test]
    async fn test_event_stream_multiple_filters() {
        let service = LoomControlService::new(":memory:".to_string());

        // Create stream with multiple filters
        let request = Request::new(EventStreamRequest {
            event_types: vec!["TICK".to_string(), "GRAPH_UPDATE".to_string()],
        });
        let mut stream = service.stream_events(request).await.unwrap().into_inner();

        // Broadcast various events
        service.broadcast_event("TICK", "tick 1");
        service.broadcast_event("OPPORTUNITY", "ignored");
        service.broadcast_event("GRAPH_UPDATE", "update 1");

        let event1 = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event1.event_type, "TICK");

        let event2 = tokio::time::timeout(tokio::time::Duration::from_millis(100), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(event2.event_type, "GRAPH_UPDATE");
    }

    #[tokio::test]
    async fn test_event_broadcast_json_data() {
        let service = LoomControlService::new(":memory:".to_string());
        let mut rx = service.event_tx.subscribe();

        let json_data = r#"{"cycle":["USD","EUR","GBP"],"profit":0.05}"#;
        service.broadcast_event("OPPORTUNITY_DETECTED", json_data);

        let event = rx.recv().await.unwrap();
        assert_eq!(event.event_type, "OPPORTUNITY_DETECTED");
        assert_eq!(event.data, json_data);

        // Verify it's valid JSON
        let _: serde_json::Value = serde_json::from_str(&event.data).unwrap();
    }

    // ===== Data Filtering Tests =====

    #[tokio::test]
    async fn test_opportunities_filter_by_start_time() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert opportunities at different timestamps
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('opp1', 1000, '[]', '', 0.01, 10.0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('opp2', 5000, '[]', '', 0.02, 10.0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('opp3', 10000, '[]', '', 0.03, 10.0, 0)",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        // Filter by start_time >= 5000
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 5000,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 2);
        assert_eq!(response.get_ref().opportunities[0].id, "opp3"); // Descending order
        assert_eq!(response.get_ref().opportunities[1].id, "opp2");

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_opportunities_filter_by_end_time() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert opportunities at different timestamps
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('opp1', 1000, '[]', '', 0.01, 10.0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('opp2', 5000, '[]', '', 0.02, 10.0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                       expected_profit, max_return_cost, gated)
             VALUES ('opp3', 10000, '[]', '', 0.03, 10.0, 0)",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        // Filter by end_time <= 5000
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 0,
            end_time: 5000,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 2);
        assert_eq!(response.get_ref().opportunities[0].id, "opp2"); // Descending order
        assert_eq!(response.get_ref().opportunities[1].id, "opp1");

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_opportunities_filter_by_time_range() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert opportunities at different timestamps
        for i in 1..=10 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?, ?, '[]', '', 0.01, 10.0, 0)",
                [&format!("opp{}", i), &(i * 1000).to_string()],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Filter by time range [3000, 7000]
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 3000,
            end_time: 7000,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 5);
        for opp in &response.get_ref().opportunities {
            assert!(opp.detected_at >= 3000);
            assert!(opp.detected_at <= 7000);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_opportunities_filter_gated_only() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert mix of gated and ungated opportunities
        for i in 0..10 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?, 1000, '[]', '', 0.01, 10.0, ?)",
                [&format!("opp{}", i), &(i % 2).to_string()],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Filter gated only
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 0,
            end_time: 0,
            gated_only: true,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 5);
        for opp in &response.get_ref().opportunities {
            assert_eq!(opp.gated, true);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_opportunities_filter_ungated_only() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert mix of gated and ungated opportunities
        for i in 0..10 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?, 1000, '[]', '', 0.01, 10.0, ?)",
                [&format!("opp{}", i), &(i % 2).to_string()],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Filter ungated only
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: true,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 5);
        for opp in &response.get_ref().opportunities {
            assert_eq!(opp.gated, false);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_opportunities_combined_filters() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert opportunities with varying timestamps and gated status
        for i in 0..20 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?, ?, '[]', '', 0.01, 10.0, ?)",
                [
                    &format!("opp{}", i),
                    &(i * 1000).to_string(),
                    &(i % 2).to_string(),
                ],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Filter: time range [5000, 15000] AND gated_only
        let request = Request::new(ListOpportunitiesRequest {
            limit: 100,
            start_time: 5000,
            end_time: 15000,
            gated_only: true,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        // Should get opportunities 5, 7, 9, 11, 13, 15 (gated and in range)
        assert_eq!(response.get_ref().opportunities.len(), 6);
        for opp in &response.get_ref().opportunities {
            assert!(opp.detected_at >= 5000);
            assert!(opp.detected_at <= 15000);
            assert_eq!(opp.gated, true);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_opportunities_limit_with_filters() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert 100 opportunities
        for i in 0..100 {
            conn.execute(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols,
                                           expected_profit, max_return_cost, gated)
                 VALUES (?, ?, '[]', '', 0.01, 10.0, 0)",
                [&format!("opp{}", i), &(i * 1000).to_string()],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Request limit of 10
        let request = Request::new(ListOpportunitiesRequest {
            limit: 10,
            start_time: 0,
            end_time: 0,
            gated_only: false,
            ungated_only: false,
        });
        let response = service.list_opportunities(request).await.unwrap();

        assert_eq!(response.get_ref().opportunities.len(), 10);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_all() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert test ticks
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (1000, 'BTC-USD', 'bid', '50000.00', '0.5')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (2000, 'ETH-USD', 'ask', '3000.00', '2.0')",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        // Export all ticks
        let request = Request::new(ExportTicksRequest {
            start_time: 0,
            end_time: 0,
            instrument: String::new(),
        });
        let response = service.export_ticks(request).await.unwrap();

        assert_eq!(response.get_ref().ticks.len(), 2);
        assert_eq!(response.get_ref().ticks[0].instrument, "BTC-USD");
        assert_eq!(response.get_ref().ticks[1].instrument, "ETH-USD");

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_by_time_range() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert ticks at different times
        for i in 1..=10 {
            conn.execute(
                "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
                 VALUES (?, 'BTC-USD', 'bid', '50000.00', '0.5')",
                [&(i * 1000)],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Export ticks in range [3000, 7000]
        let request = Request::new(ExportTicksRequest {
            start_time: 3000,
            end_time: 7000,
            instrument: String::new(),
        });
        let response = service.export_ticks(request).await.unwrap();

        assert_eq!(response.get_ref().ticks.len(), 5);
        for tick in &response.get_ref().ticks {
            assert!(tick.timestamp >= 3000);
            assert!(tick.timestamp <= 7000);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_by_instrument() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert ticks for different instruments
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (1000, 'BTC-USD', 'bid', '50000.00', '0.5')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (2000, 'ETH-USD', 'ask', '3000.00', '2.0')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (3000, 'BTC-USD', 'ask', '50100.00', '0.3')",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        // Export only BTC-USD ticks
        let request = Request::new(ExportTicksRequest {
            start_time: 0,
            end_time: 0,
            instrument: "BTC-USD".to_string(),
        });
        let response = service.export_ticks(request).await.unwrap();

        assert_eq!(response.get_ref().ticks.len(), 2);
        for tick in &response.get_ref().ticks {
            assert_eq!(tick.instrument, "BTC-USD");
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_combined_filters() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert many ticks with different instruments and timestamps
        for i in 1..=20 {
            let instrument = if i % 2 == 0 { "BTC-USD" } else { "ETH-USD" };
            conn.execute(
                "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
                 VALUES (?, ?, 'bid', '50000.00', '0.5')",
                rusqlite::params![i * 1000, instrument],
            )
            .unwrap();
        }

        let service = LoomControlService::new(db_path.clone());

        // Export BTC-USD ticks in time range [5000, 15000]
        let request = Request::new(ExportTicksRequest {
            start_time: 5000,
            end_time: 15000,
            instrument: "BTC-USD".to_string(),
        });
        let response = service.export_ticks(request).await.unwrap();

        // Should get ticks at timestamps 6000, 8000, 10000, 12000, 14000
        assert_eq!(response.get_ref().ticks.len(), 5);
        for tick in &response.get_ref().ticks {
            assert_eq!(tick.instrument, "BTC-USD");
            assert!(tick.timestamp >= 5000);
            assert!(tick.timestamp <= 15000);
        }

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_empty_result() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert tick outside the filter range
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (1000, 'BTC-USD', 'bid', '50000.00', '0.5')",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        // Request ticks in a range with no data
        let request = Request::new(ExportTicksRequest {
            start_time: 5000,
            end_time: 10000,
            instrument: String::new(),
        });
        let response = service.export_ticks(request).await.unwrap();

        assert_eq!(response.get_ref().ticks.len(), 0);

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_data_integrity() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Insert tick with specific values
        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (12345678, 'BTC-USD', 'bid', '50123.45', '1.23456789')",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        let request = Request::new(ExportTicksRequest {
            start_time: 0,
            end_time: 0,
            instrument: String::new(),
        });
        let response = service.export_ticks(request).await.unwrap();

        assert_eq!(response.get_ref().ticks.len(), 1);
        let tick = &response.get_ref().ticks[0];
        assert_eq!(tick.timestamp, 12345678);
        assert_eq!(tick.instrument, "BTC-USD");
        assert_eq!(tick.side, "bid");
        assert_eq!(tick.price, "50123.45");
        assert_eq!(tick.quantity, "1.23456789");

        cleanup_test_db(&db_path);
    }

    #[tokio::test]
    async fn test_export_ticks_json_serialization() {
        let db_path = create_test_db();
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        conn.execute(
            "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
             VALUES (1000, 'BTC-USD', 'bid', '50000.00', '0.5')",
            [],
        )
        .unwrap();

        let service = LoomControlService::new(db_path.clone());

        let request = Request::new(ExportTicksRequest {
            start_time: 0,
            end_time: 0,
            instrument: String::new(),
        });
        let response = service.export_ticks(request).await.unwrap();

        // Verify JSON serialization works
        let json = serde_json::to_string(&response.get_ref()).unwrap();
        assert!(json.contains("BTC-USD"));
        assert!(json.contains("50000.00"));

        // Verify all fields are present in JSON
        assert!(json.contains("timestamp"));
        assert!(json.contains("instrument"));
        assert!(json.contains("side"));
        assert!(json.contains("price"));
        assert!(json.contains("quantity"));

        cleanup_test_db(&db_path);
    }
}
