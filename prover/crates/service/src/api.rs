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
}
