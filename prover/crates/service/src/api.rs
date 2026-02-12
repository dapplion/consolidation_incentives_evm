//! REST API Endpoints
//!
//! Health, status, and consolidation query endpoints.

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;

/// Run the API server
pub async fn run_server(listen: String, state: AppState) -> anyhow::Result<()> {
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!(address = %listen, "API server listening");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the API router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/consolidations", get(list_consolidations))
        .route("/consolidations/{source_index}", get(get_consolidation))
        .route("/metrics", get(metrics))
        .with_state(state)
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    slots_behind: u64,
}

/// Health check endpoint
async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let healthy = state.is_healthy();
    let status_code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let response = HealthResponse {
        status: if healthy { "healthy" } else { "degraded" },
        slots_behind: state.slots_behind(),
    };

    (status_code, Json(response))
}

/// Status response
#[derive(Serialize)]
struct StatusResponse {
    current_slot: u64,
    current_epoch: u64,
    head_slot: u64,
    slots_behind: u64,
    uptime_secs: u64,
    consolidations: crate::state::StatusCounts,
    last_error: Option<String>,
}

/// Status endpoint
async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        current_slot: state.current_slot(),
        current_epoch: state.current_epoch(),
        head_slot: state.head_slot(),
        slots_behind: state.slots_behind(),
        uptime_secs: state.uptime_secs(),
        consolidations: state.status_counts(),
        last_error: state.last_error(),
    })
}

/// List all consolidations
async fn list_consolidations(
    State(state): State<AppState>,
) -> Json<Vec<crate::state::ConsolidationRecord>> {
    Json(state.all_consolidations())
}

/// Get a single consolidation by source index
async fn get_consolidation(
    State(state): State<AppState>,
    Path(source_index): Path<u64>,
) -> Result<Json<crate::state::ConsolidationRecord>, StatusCode> {
    state
        .get_consolidation(source_index)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Prometheus metrics endpoint
async fn metrics(State(state): State<AppState>) -> String {
    use metrics::{describe_counter, describe_gauge, describe_histogram};

    // Register metric descriptions
    describe_gauge!("sync_current_slot", "Current finalized slot");
    describe_gauge!("sync_slots_behind", "Number of slots behind head");
    describe_counter!(
        "consolidations_detected_total",
        "Total consolidations detected"
    );
    describe_counter!(
        "consolidations_submitted_total",
        "Total consolidation claims submitted"
    );
    describe_counter!(
        "consolidations_confirmed_total",
        "Total consolidation claims confirmed"
    );
    describe_counter!(
        "consolidations_failed_total",
        "Total consolidation claims failed"
    );
    describe_histogram!(
        "proof_generation_duration_seconds",
        "Time to generate proofs"
    );
    describe_histogram!("tx_submission_duration_seconds", "Time to submit transaction");

    // Update gauge values from state
    metrics::gauge!("sync_current_slot").set(state.current_slot() as f64);
    metrics::gauge!("sync_slots_behind").set(state.slots_behind() as f64);

    let counts = state.status_counts();
    metrics::gauge!("consolidations_detected_count").set(counts.detected as f64);
    metrics::gauge!("consolidations_proof_built_count").set(counts.proof_built as f64);
    metrics::gauge!("consolidations_submitted_count").set(counts.submitted as f64);
    metrics::gauge!("consolidations_confirmed_count").set(counts.confirmed as f64);
    metrics::gauge!("consolidations_failed_count").set(counts.failed as f64);

    // Export in Prometheus text format
    // Note: This is a simplified implementation
    // Full production would use metrics-exporter-prometheus PrometheusBuilder
    format!(
        "# HELP sync_current_slot Current finalized slot\n\
         # TYPE sync_current_slot gauge\n\
         sync_current_slot {}\n\
         # HELP sync_slots_behind Number of slots behind head\n\
         # TYPE sync_slots_behind gauge\n\
         sync_slots_behind {}\n\
         # HELP consolidations_detected_count Consolidations in detected state\n\
         # TYPE consolidations_detected_count gauge\n\
         consolidations_detected_count {}\n\
         # HELP consolidations_proof_built_count Consolidations with proofs built\n\
         # TYPE consolidations_proof_built_count gauge\n\
         consolidations_proof_built_count {}\n\
         # HELP consolidations_submitted_count Consolidations submitted on-chain\n\
         # TYPE consolidations_submitted_count gauge\n\
         consolidations_submitted_count {}\n\
         # HELP consolidations_confirmed_count Consolidations confirmed on-chain\n\
         # TYPE consolidations_confirmed_count gauge\n\
         consolidations_confirmed_count {}\n\
         # HELP consolidations_failed_count Failed consolidation claims\n\
         # TYPE consolidations_failed_count gauge\n\
         consolidations_failed_count {}\n",
        state.current_slot(),
        state.slots_behind(),
        counts.detected,
        counts.proof_built,
        counts.submitted,
        counts.confirmed,
        counts.failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic test that router creation works
    #[test]
    fn test_create_router() {
        let state = AppState::new();
        let _router = create_router(state);
    }

    // Test the health logic directly
    #[test]
    fn test_health_logic() {
        let state = AppState::new();
        state.set_head_slot(100);
        state.set_current_slot(100);
        assert!(state.is_healthy());

        state.set_head_slot(200);
        assert!(!state.is_healthy()); // 100 slots behind
    }

    // Test health response construction
    #[tokio::test]
    async fn test_health_response_healthy() {
        let state = AppState::new();
        state.set_head_slot(50);
        state.set_current_slot(50);

        let (status_code, Json(response)) = health(State(state)).await;

        assert_eq!(status_code, StatusCode::OK);
        assert_eq!(response.status, "healthy");
        assert_eq!(response.slots_behind, 0);
    }

    #[tokio::test]
    async fn test_health_response_degraded() {
        let state = AppState::new();
        state.set_head_slot(200);
        state.set_current_slot(100);

        let (status_code, Json(response)) = health(State(state)).await;

        assert_eq!(status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.status, "degraded");
        assert_eq!(response.slots_behind, 100);
    }

    #[tokio::test]
    async fn test_status_response() {
        let state = AppState::new();
        state.set_current_slot(100);
        state.set_current_epoch(6);
        state.set_head_slot(120);

        let Json(response) = status(State(state)).await;

        assert_eq!(response.current_slot, 100);
        assert_eq!(response.current_epoch, 6);
        assert_eq!(response.head_slot, 120);
        assert_eq!(response.slots_behind, 20);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let state = AppState::new();
        state.set_current_slot(100);
        state.set_head_slot(150);

        let output = metrics(State(state)).await;

        assert!(output.contains("sync_current_slot 100"));
        assert!(output.contains("sync_slots_behind 50"));
        assert!(output.contains("consolidations_detected_count"));
    }
}
