//! Beacon API HTTP Client
//!
//! Fetches beacon state data from a Gnosis beacon node.

use crate::types::{BeaconBlockHeader, FinalityCheckpoints, PendingConsolidationJson, ValidatorInfo};
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

    /// Fetch pending consolidations for a given state
    ///
    /// Uses the standard (non-debug) Beacon API endpoint introduced in Electra:
    /// `GET /eth/v1/beacon/states/{state_id}/pending_consolidations`
    ///
    /// # Errors
    /// Returns error if the request fails or response is invalid
    #[instrument(skip(self))]
    pub async fn get_pending_consolidations(
        &self,
        state_id: &str,
    ) -> Result<Vec<PendingConsolidationJson>, BeaconClientError> {
        let url = format!(
            "{}/eth/v1/beacon/states/{state_id}/pending_consolidations",
            self.base_url
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BeaconClientError::InvalidResponse(format!(
                "pending_consolidations not found for state_id={state_id}"
            )));
        }

        if !response.status().is_success() {
            return Err(BeaconClientError::InvalidResponse(format!(
                "Unexpected status: {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct PendingConsolidationsResponse {
            data: Vec<PendingConsolidationEntry>,
        }

        #[derive(Deserialize)]
        struct PendingConsolidationEntry {
            source_index: String,
            target_index: String,
        }

        let resp: PendingConsolidationsResponse = response.json().await?;

        let mut out = Vec::with_capacity(resp.data.len());
        for entry in resp.data {
            out.push(PendingConsolidationJson {
                source_index: entry.source_index.parse().map_err(|e| {
                    BeaconClientError::InvalidResponse(format!(
                        "Invalid source_index: {e}"
                    ))
                })?,
                target_index: entry.target_index.parse().map_err(|e| {
                    BeaconClientError::InvalidResponse(format!(
                        "Invalid target_index: {e}"
                    ))
                })?,
            });
        }

        Ok(out)
    }

    /// Fetch minimal validator info for a given state and validator index
    ///
    /// `GET /eth/v1/beacon/states/{state_id}/validators/{validator_id}`
    ///
    /// # Errors
    /// Returns error if request fails or response is invalid
    #[instrument(skip(self))]
    pub async fn get_validator_info(
        &self,
        state_id: &str,
        validator_id: u64,
    ) -> Result<ValidatorInfo, BeaconClientError> {
        let url = format!(
            "{}/eth/v1/beacon/states/{state_id}/validators/{validator_id}",
            self.base_url
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(BeaconClientError::InvalidResponse(format!(
                "validator {validator_id} not found for state_id={state_id}"
            )));
        }

        if !response.status().is_success() {
            return Err(BeaconClientError::InvalidResponse(format!(
                "Unexpected status: {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct ValidatorResponse {
            data: ValidatorData,
        }

        #[derive(Deserialize)]
        struct ValidatorData {
            validator: ValidatorInner,
        }

        #[derive(Deserialize)]
        struct ValidatorInner {
            withdrawal_credentials: String,
            activation_epoch: String,
        }

        let resp: ValidatorResponse = response.json().await?;

        Ok(ValidatorInfo {
            withdrawal_credentials: parse_hex32(&resp.data.validator.withdrawal_credentials)?,
            activation_epoch: resp
                .data
                .validator
                .activation_epoch
                .parse()
                .map_err(|e| {
                    BeaconClientError::InvalidResponse(format!(
                        "Invalid activation_epoch: {e}"
                    ))
                })?,
        })
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

    #[tokio::test]
    async fn test_get_state_ssz() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path, header};

        let mock_server = MockServer::start().await;
        
        // Mock SSZ state response
        let ssz_data = vec![0x01, 0x02, 0x03, 0x04];
        Mock::given(method("GET"))
            .and(path("/eth/v2/debug/beacon/states/12345"))
            .and(header("Accept", "application/octet-stream"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(ssz_data.clone()))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let result = client.get_state_ssz("12345").await.unwrap();
        
        assert_eq!(result, ssz_data);
    }

    #[tokio::test]
    async fn test_get_state_ssz_not_found() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        
        Mock::given(method("GET"))
            .and(path("/eth/v2/debug/beacon/states/99999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let result = client.get_state_ssz("99999").await;
        
        assert!(matches!(result, Err(BeaconClientError::StateNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_header() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        
        let response_json = r#"{
            "data": {
                "header": {
                    "message": {
                        "slot": "12345",
                        "proposer_index": "42",
                        "parent_root": "0x0101010101010101010101010101010101010101010101010101010101010101",
                        "state_root": "0x0202020202020202020202020202020202020202020202020202020202020202",
                        "body_root": "0x0303030303030303030303030303030303030303030303030303030303030303"
                    }
                }
            }
        }"#;
        
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/headers/12345"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let header = client.get_header("12345").await.unwrap();
        
        assert_eq!(header.slot, 12345);
        assert_eq!(header.proposer_index, 42);
        assert_eq!(header.parent_root[0], 0x01);
        assert_eq!(header.state_root[0], 0x02);
        assert_eq!(header.body_root[0], 0x03);
    }

    #[tokio::test]
    async fn test_get_header_not_found() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/headers/99999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let result = client.get_header("99999").await;
        
        assert!(matches!(result, Err(BeaconClientError::HeaderNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_finality_checkpoints() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        
        let response_json = r#"{
            "data": {
                "previous_justified": {
                    "epoch": "100",
                    "root": "0x0101010101010101010101010101010101010101010101010101010101010101"
                },
                "current_justified": {
                    "epoch": "101",
                    "root": "0x0202020202020202020202020202020202020202020202020202020202020202"
                },
                "finalized": {
                    "epoch": "99",
                    "root": "0x0303030303030303030303030303030303030303030303030303030303030303"
                }
            }
        }"#;
        
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/states/head/finality_checkpoints"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let checkpoints = client.get_finality_checkpoints().await.unwrap();
        
        assert_eq!(checkpoints.previous_justified_epoch, 100);
        assert_eq!(checkpoints.current_justified_epoch, 101);
        assert_eq!(checkpoints.finalized_epoch, 99);
        assert_eq!(checkpoints.finalized_root[0], 0x03);
    }

    #[tokio::test]
    async fn test_get_head_slot() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        
        let response_json = r#"{
            "data": {
                "header": {
                    "message": {
                        "slot": "54321",
                        "proposer_index": "7",
                        "parent_root": "0x0101010101010101010101010101010101010101010101010101010101010101",
                        "state_root": "0x0202020202020202020202020202020202020202020202020202020202020202",
                        "body_root": "0x0303030303030303030303030303030303030303030303030303030303030303"
                    }
                }
            }
        }"#;
        
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/headers/head"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let slot = client.get_head_slot().await.unwrap();
        
        assert_eq!(slot, 54321);
    }

    #[tokio::test]
    async fn test_get_header_invalid_json() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let mock_server = MockServer::start().await;
        
        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/headers/12345"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid json"))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let result = client.get_header("12345").await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_pending_consolidations() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let response_json = r#"{
            "data": [
                {"source_index": "42", "target_index": "100"},
                {"source_index": "7", "target_index": "8"}
            ]
        }"#;

        Mock::given(method("GET"))
            .and(path(
                "/eth/v1/beacon/states/12345/pending_consolidations",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let result = client.get_pending_consolidations("12345").await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source_index, 42);
        assert_eq!(result[0].target_index, 100);
        assert_eq!(result[1].source_index, 7);
        assert_eq!(result[1].target_index, 8);
    }

    #[tokio::test]
    async fn test_get_validator_info() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let response_json = r#"{
            "data": {
                "index": "42",
                "balance": "32000000000",
                "status": "active_ongoing",
                "validator": {
                    "pubkey": "0x00",
                    "withdrawal_credentials": "0x0101010101010101010101010101010101010101010101010101010101010101",
                    "effective_balance": "32000000000",
                    "slashed": false,
                    "activation_eligibility_epoch": "0",
                    "activation_epoch": "123",
                    "exit_epoch": "18446744073709551615",
                    "withdrawable_epoch": "18446744073709551615"
                }
            }
        }"#;

        Mock::given(method("GET"))
            .and(path("/eth/v1/beacon/states/finalized/validators/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = BeaconClient::new(mock_server.uri());
        let info = client.get_validator_info("finalized", 42).await.unwrap();

        assert_eq!(info.activation_epoch, 123);
        assert_eq!(info.withdrawal_credentials[0], 0x01);
    }
}
