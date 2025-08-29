/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/providers/sealed_secrets.rs
* This file provides the concrete implementation of the `SecretProvider` trait
* for Bitnami's Sealed Secrets. This provider operates by interacting with the
* Kubernetes API to read the standard `Secret` object that is automatically
* created and decrypted by the Sealed Secrets controller in the cluster. It does
* not perform decryption itself, but rather leverages the output of the
* in-cluster operator, which is the correct pattern for this tool.
* SPDX-License-Identifier: Apache-2.0 */

use super::SecretProvider;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use serde::Deserialize;

// --- Configuration ---

/// Configuration for the Sealed Secrets provider. This is empty because all
/// necessary configuration is inferred from the environment by the kube client
/// (e.g., kubeconfig file or in-cluster service account).
#[derive(Deserialize, Debug)]
pub struct SealedSecretsConfig {}

// --- Provider Implementation ---

/// A `SecretProvider` for fetching values from decrypted Sealed Secrets.
pub struct SealedSecretsProvider;

impl SealedSecretsProvider {
    /// Creates a new `SealedSecretsProvider`.
    pub fn new(_config: SealedSecretsConfig) -> Result<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl SecretProvider for SealedSecretsProvider {
    /// Fetches a secret value from a Kubernetes `Secret` managed by Sealed Secrets.
    ///
    /// The `key` is expected to be in the format: `namespace/secret-name:key-in-data`.
    /// For example: `production/my-app-db-creds:password`.
    async fn fetch_secret_value(&self, key: &str) -> Result<String> {
        let (namespace_and_name, data_key) = key.split_once(':').ok_or_else(|| {
            anyhow!(
                "Invalid Sealed Secrets key format. Expected 'namespace/name:key', got '{}'",
                key
            )
        })?;

        let (namespace, secret_name) = namespace_and_name.split_once('/').ok_or_else(|| {
            anyhow!(
                "Invalid Sealed Secrets namespace/name format. Expected 'namespace/name', got '{}'",
                namespace_and_name
            )
        })?;

        println!(
            "Fetching key '{}' from Secret '{}' in namespace '{}'...",
            data_key, secret_name, namespace
        );

        // Initialize a Kubernetes client from the environment.
        let client = Client::try_default()
            .await
            .context("Failed to create Kubernetes client")?;

        // Get an API handle for `Secret` resources in the specified namespace.
        let api: Api<Secret> = Api::namespaced(client, namespace);

        // Fetch the Secret object from the cluster.
        let secret = api
            .get(secret_name)
            .await
            .with_context(|| format!("Failed to get Secret '{}' in namespace '{}'", secret_name, namespace))?;

        // Extract the .data field, which contains the decrypted, base64-encoded values.
        let data = secret.data.ok_or_else(|| {
            anyhow!("Secret '{}' does not contain a '.data' field", secret_name)
        })?;

        // Look up the specific key and decode its base64 value.
        let encoded_value = data.get(data_key).ok_or_else(|| {
            anyhow!(
                "Key '{}' not found in the data of Secret '{}'",
                data_key,
                secret_name
            )
        })?;

        let decoded_bytes = B64
            .decode(&encoded_value.0)
            .with_context(|| format!("Value for key '{}' is not valid Base64", data_key))?;

        // Convert the raw bytes to a UTF-8 string.
        String::from_utf8(decoded_bytes)
            .with_context(|| format!("Decoded value for key '{}' is not valid UTF-8", data_key))
    }
}