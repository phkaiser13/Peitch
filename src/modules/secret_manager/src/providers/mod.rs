/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/providers/mod.rs
*
* This file defines the core abstraction for the secret_manager module. It
* establishes a generic `SecretProvider` trait that defines a common contract
* for interacting with secrets, regardless of the backend. This contract includes
* operations like fetching and rotating secrets.
*
* It also includes a factory function, `get_provider`, which instantiates the
* correct concrete provider based on a strongly-typed configuration enum. This
* design decouples the main orchestration logic from the specific details of
* each secret provider, enabling extreme modularity and extensibility.
*
* SPDX-License-Identifier: Apache-2.0 */

pub mod sealed_secrets;
pub mod sops;
pub mod vault;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

// --- Configuration Structures ---

/// A tagged enum representing the configuration for any supported secret provider.
/// Using `#[serde(tag = "provider", rename_all = "snake_case")]` allows for clean
/// and type-safe JSON configuration.
#[derive(Deserialize, Debug)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderConfig {
    Vault(vault::VaultConfig),
    Sops(sops::SopsConfig),
    SealedSecrets(sealed_secrets::SealedSecretsConfig),
}

// --- Abstraction Trait ---

/// The `SecretProvider` trait defines the universal contract for all secret backends.
/// Any struct that implements this trait can be used by the orchestrator to fetch
/// or rotate secret values.
#[async_trait]
pub trait SecretProvider {
    /// Fetches a single secret value from the backend given a specific key.
    /// The format of the key is specific to the provider (e.g., a path in Vault,
    /// or a file path + key for SOPS).
    ///
    /// # Arguments
    /// * `key` - The provider-specific identifier for the secret to fetch.
    ///
    /// # Returns
    /// A `Result` containing the raw secret value as a `String`.
    async fn fetch_secret_value(&self, key: &str) -> Result<String>;

    /// Triggers the rotation of a secret in the backend.
    /// The implementation is specific to each provider. For some providers like
    /// SOPS, this might be a no-op, while for others like Vault with dynamic
    /// secrets, it will trigger a specific API endpoint.
    ///
    /// # Arguments
    /// * `key` - The provider-specific identifier for the secret to rotate.
    ///
    /// # Returns
    /// An empty `Result` indicating success or failure.
    async fn rotate_secret_value(&self, key: &str, force: bool) -> Result<()>;
}

// --- Factory Function ---

/// Instantiates and returns a concrete `SecretProvider` based on the provided config.
/// This factory function is the bridge between the configuration and the implementation,
/// allowing the main logic to remain decoupled.
///
/// # Arguments
/// * `config` - The `ProviderConfig` enum variant specifying which provider to use.
///
/// # Returns
/// A `Result` containing a boxed trait object of the requested provider.
pub fn get_provider(config: ProviderConfig) -> Result<Box<dyn SecretProvider + Send + Sync>> {
    match config {
        ProviderConfig::Vault(conf) => {
            let provider = vault::VaultProvider::new(conf)?;
            Ok(Box::new(provider))
        }
        ProviderConfig::Sops(conf) => {
            let provider = sops::SopsProvider::new(conf)?;
            Ok(Box::new(provider))
        }
        ProviderConfig::SealedSecrets(conf) => {
            let provider = sealed_secrets::SealedSecretsProvider::new(conf)?;
            Ok(Box::new(provider))
        }
    }
}