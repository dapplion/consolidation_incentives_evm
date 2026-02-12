//! Beacon API HTTP Client
//!
//! Fetches beacon state data from a Gnosis beacon node.

use crate::types::{BeaconBlockHeader, FinalityCheckpoints};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tracing::instrument;

/// Errors from beacon API operations
#[derive(Debug, Error)]
pub enum BeaconClientError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("State not found for slot {0}")]
    StateNotFound(u64),

    #[error("Header not found for slot {0}")]
    HeaderNotFound(u64),
}

/// Client for interacting with the Beacon API
#[derive(Debug, Clone)]
pub struct BeaconClient {
    client: Client,
    base_url: String,
}

impl BeaconClient {
    /// Create a new beacon client
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the beacon node (e.g., `http://localhost:5052`)
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Fetch beacon state as SSZ bytes
    ///
    /// # Arguments
    /// * `state_id` - State identifier (slot number, "head", "finalized", etc.)
    ///
    /// # Errors
    /// Returns error if the request fails or state is not found
    #[instrument(skip(self))]
    pub async fn get_state_ssz(&self, state_id: &str) -> Result<Vec<u8>, BeaconClientError> {
        let url = format!("{}/eth/v2/debug/beacon/states/{state_id}", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/octet-stream")
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BeaconClientError::StateNotFound(
                state_id.parse().unwrap_or(0),
            ));
        }

        if !response.status().is_success() {
            return Err(BeaconClientError::InvalidResponse(format!(
                "Unexpected status: {}",
                response.status()
            )));
        }

        Ok(response.bytes().await?.to_vec())
    }

    /// Fetch beacon block header
    ///
    /// # Arguments
    /// * `block_id` - Block identifier (slot number, "head", "finalized", etc.)
    ///
    /// # Errors
    /// Returns error if the request fails or header is not found
    #[instrument(skip(self))]
    pub async fn get_header(&self, block_id: &str) -> Result<BeaconBlockHeader, BeaconClientError> {
        let url = format!("{}/eth/v1/beacon/headers/{block_id}", self.base_url);

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BeaconClientError::HeaderNotFound(
                block_id.parse().unwrap_or(0),
            ));
        }

        #[derive(Deserialize)]
        struct HeaderResponse {
            data: HeaderData,
        }

        #[derive(Deserialize)]
        struct HeaderData {
            header: HeaderMessage,
        }

        #[derive(Deserialize)]
        struct HeaderMessage {
            message: BeaconBlockHeaderJson,
        }

        #[derive(Deserialize)]
        struct BeaconBlockHeaderJson {
            slot: String,
            proposer_index: String,
            parent_root: String,
            state_root: String,
            body_root: String,
        }

        let header_resp: HeaderResponse = response.json().await?;
        let msg = header_resp.data.header.message;

        Ok(BeaconBlockHeader {
            slot: msg.slot.parse().map_err(|e| {
                BeaconClientError::InvalidResponse(format!("Invalid slot: {e}"))
            })?,
            proposer_index: msg.proposer_index.parse().map_err(|e| {
                BeaconClientError::InvalidResponse(format!("Invalid proposer_index: {e}"))
            })?,
            parent_root: parse_hex32(&msg.parent_root)?,
            state_root: parse_hex32(&msg.state_root)?,
            body_root: parse_hex32(&msg.body_root)?,
        })
    }

    /// Fetch finality checkpoints
    ///
    /// # Errors
    /// Returns error if the request fails
    #[instrument(skip(self))]
    pub async fn get_finality_checkpoints(&self) -> Result<FinalityCheckpoints, BeaconClientError> {
        let url = format!(
            "{}/eth/v1/beacon/states/head/finality_checkpoints",
            self.base_url
        );

        let response = self.client.get(&url).send().await?;

        #[derive(Deserialize)]
        struct CheckpointsResponse {
            data: CheckpointsData,
        }

        #[derive(Deserialize)]
        struct CheckpointsData {
            previous_justified: Checkpoint,
            current_justified: Checkpoint,
            finalized: Checkpoint,
        }

        #[derive(Deserialize)]
        struct Checkpoint {
            epoch: String,
            root: String,
        }

        let resp: CheckpointsResponse = response.json().await?;

        Ok(FinalityCheckpoints {
            previous_justified_epoch: resp.data.previous_justified.epoch.parse().map_err(|e| {
                BeaconClientError::InvalidResponse(format!("Invalid epoch: {e}"))
            })?,
            current_justified_epoch: resp.data.current_justified.epoch.parse().map_err(|e| {
                BeaconClientError::InvalidResponse(format!("Invalid epoch: {e}"))
            })?,
            finalized_epoch: resp.data.finalized.epoch.parse().map_err(|e| {
                BeaconClientError::InvalidResponse(format!("Invalid epoch: {e}"))
            })?,
            finalized_root: parse_hex32(&resp.data.finalized.root)?,
        })
    }

    /// Get current head slot
    ///
    /// # Errors
    /// Returns error if the request fails
    pub async fn get_head_slot(&self) -> Result<u64, BeaconClientError> {
        let header = self.get_header("head").await?;
        Ok(header.slot)
    }
}

fn parse_hex32(s: &str) -> Result<[u8; 32], BeaconClientError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s)
        .map_err(|e| BeaconClientError::InvalidResponse(format!("Invalid hex: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| BeaconClientError::InvalidResponse("Expected 32 bytes".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex32() {
        let hex = "0x0102030405060708091011121314151617181920212223242526272829303132";
        let result = parse_hex32(hex).unwrap();
        assert_eq!(result[0], 0x01);
        assert_eq!(result[31], 0x32);
    }

    #[test]
    fn test_parse_hex32_without_prefix() {
        let hex = "0102030405060708091011121314151617181920212223242526272829303132";
        let result = parse_hex32(hex).unwrap();
        assert_eq!(result[0], 0x01);
    }

    #[test]
    fn test_parse_hex32_invalid_length() {
        let hex = "0x0102";
        assert!(parse_hex32(hex).is_err());
    }
}
