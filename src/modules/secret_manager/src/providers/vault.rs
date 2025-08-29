/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/secret_manager/src/providers/vault.rs
*
* This file provides the concrete implementation of the `SecretProvider` trait for
* HashiCorp Vault. It handles the specifics of communicating with the Vault API
* for both fetching and rotating secrets.
*
* Architecture:
* The implementation uses the `reqwest` crate for all HTTP communication, ensuring
* a consistent and powerful asynchronous networking layer.
*
* Key components:
* 1. `VaultConfig`: A deserializable struct holding the necessary credentials for
*    connecting to Vault—the server address and the authentication token.
* 2. `VaultProvider`: The main struct that encapsulates the configuration and the
*    `reqwest` client. It implements the `SecretProvider` trait.
* 3. `fetch_secret_value`: Implements the logic for reading a specific field from a
*    secret within Vault's KVv2 engine. It expects a key in the format
*    `path/to/secret:field_name`.
* 4. `rotate_secret_value`: Implements the logic for triggering a credential
*    rotation. This is typically used with Vault's database secrets engine, where
*    making a POST request to a specific endpoint invalidates the old credential
*    and generates a new one.
*
* All operations are fully asynchronous and return `anyhow::Result` for rich,
* contextual error handling, simplifying debugging of network, permission, or
* configuration issues.
*
* SPDX-License-Identifier: Apache-2.0 */

use crate::providers::SecretProvider;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

// --- Configuration ---

/// Configuration for the Vault provider.
/// This struct is deserialized from the JSON payload sent by the C core.
#[derive(Deserialize, Debug)]
pub struct VaultConfig {
    /// The network address of the Vault server (e.g., "http://127.0.0.1:8200").
    pub address: String,
    /// The authentication token to be used for all API requests.
    pub token: String,
}

// --- Provider Implementation ---

/// A provider for fetching and managing secrets from HashiCorp Vault.
pub struct VaultProvider {
    config: VaultConfig,
    client: reqwest::Client,
}

impl VaultProvider {
    /// Creates a new `VaultProvider`.
    ///
    /// # Arguments
    /// * `config` - A `VaultConfig` struct containing the address and token.
    ///
    /// # Returns
    /// A new instance of `VaultProvider`.
    pub fn new(config: VaultConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()
            .context("Failed to build reqwest HTTP client for Vault provider")?;
        Ok(Self { config, client })
    }
}

#[async_trait]
impl SecretProvider for VaultProvider {
    /// Fetches a single secret value from the Vault KVv2 engine.
    ///
    /// The `key` must be in the format "path/to/secret:field_name".
    /// For example: "secret/data/my-app/db:password".
    async fn fetch_secret_value(&self, key: &str) -> Result<String> {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        if parts.len() != 2 {
            bail!(
                "Invalid secret key format for Vault. Expected 'path:field', got '{}'",
                key
            );
        }
        let path = parts[0];
        let field = parts[1];

        let url = format!("{}/v1/{}", self.config.address, path);

        log::debug!("Fetching secret from Vault URL: {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.config.token)
            .send()
            .await
            .with_context(|| format!("Failed to send GET request to Vault at '{}'", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body.".to_string());
            bail!(
                "Vault API returned an error for path '{}'. Status: {}. Body: {}",
                path,
                status,
                error_body
            );
        }

        let json: Value = response
            .json()
            .await
            .with_context(|| format!("Failed to parse JSON response from Vault for path '{}'", path))?;

        // For KVv2, the data is nested under `data.data`.
        let secret_value = json
            .get("data")
            .and_then(|d| d.get("data"))
            .and_then(|d| d.get(field))
            .and_then(|v| v.as_str())
            .with_context(|| {
                format!(
                    "Field '{}' not found in secret at path '{}' or value is not a string. Check the path and secret structure.",
                    field, path
                )
            })?
            .to_string();

        Ok(secret_value)
    }

    /// Triggers the rotation of a secret in the backend.
    ///
    /// For Vault, the `key` is the path to the rotation endpoint.
    /// Example: "database/rotate-root/my-webapp-role"
    async fn rotate_secret_value(&self, key: &str, force: bool) -> Result<()> {
        log::info!("Rotating Vault credential at path: {} (Force: {})", key, force);

        // The Vault API endpoint for rotating DB credentials is typically a POST
        // to /v1/<mount_path>/rotate-root/<role_name>
        let url = format!("{}/v1/{}", self.config.address, key);
        
        let mut request_builder = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.config.token);

        // Some Vault rotation endpoints might accept a 'force' parameter in the body.
        if force {
            request_builder = request_builder.json(&serde_json::json!({ "force": true }));
        }

        let response = request_builder
            .send()
            .await
            .with_context(|| format!("Failed to send rotation request to Vault at '{}'", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body.".to_string());
            bail!(
                "Vault API returned an error during rotation at '{}'. Status: {}. Body: {}",
                key,
                status,
                error_body
            );
        }

        log::info!(
            "Rotation request for '{}' sent successfully to Vault.",
            key
        );
        Ok(())
    }
}