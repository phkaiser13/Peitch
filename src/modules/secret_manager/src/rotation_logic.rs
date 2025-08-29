/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/secret_manager/src/rotation_logic.rs
*
* This file contains the high-level orchestration logic for the secret rotation
* feature. It acts as the bridge between the FFI layer and the provider-specific
* implementations.
*
* Its primary responsibilities are:
* 1. Defining the `RotateRequest` struct, which represents the deserialized JSON
*    payload received from the C core. This provides a strongly-typed
*    representation of the rotation command.
* 2. Implementing the `run_rotation_internal` async function, which is the core
*    of the rotation workflow. This function takes a `RotateRequest`, uses the
*    `get_provider` factory to instantiate the correct secret provider, and then
*    invokes the `rotate_secret_value` method on that provider.
*
* By centralizing this logic, we keep the FFI entry point (`lib.rs`) clean and
* focused solely on interfacing with C, while the provider modules remain focused
* on their specific backend interactions.
*
* SPDX-License-Identifier: Apache-2.0 */

use crate::providers::{get_provider, ProviderConfig};
use anyhow::{Context, Result};
use serde::Deserialize;

/// Represents the JSON request payload for a secret rotation operation.
/// This structure is deserialized from the JSON string passed via the FFI.
#[derive(Deserialize, Debug)]
pub struct RotateRequest {
    /// The configuration for the specific provider (e.g., Vault, SOPS).
    pub provider: ProviderConfig,
    /// The provider-specific path or key for the secret to be rotated.
    pub path: String,
    /// Whether to force the rotation without confirmation.
    #[serde(default)]
    pub force: bool,
}

/// The core asynchronous logic for handling a secret rotation request.
///
/// This function orchestrates the rotation process by creating the appropriate
/// provider and calling its rotation method.
///
/// # Arguments
/// * `request` - A `RotateRequest` containing all necessary information.
///
/// # Returns
/// An empty `Result` indicating success or failure of the operation.
pub async fn run_rotation_internal(request: RotateRequest) -> Result<()> {
    log::info!(
        "Initiating secret rotation for path '{}' (Force: {})...",
        request.path,
        request.force
    );

    // 1. Instantiate the configured provider using the factory.
    // This dynamically selects the correct implementation (e.g., VaultProvider)
    // based on the content of the JSON payload.
    let provider =
        get_provider(request.provider).context("Failed to initialize the secret provider")?;

    // 2. Call the provider's rotation function.
    // The actual implementation of this method is specific to the provider.
    provider
        .rotate_secret_value(&request.path, request.force)
        .await
        .with_context(|| format!("Failed to execute rotation for path '{}'", request.path))?;

    log::info!(
        "Rotation for path '{}' completed successfully.",
        request.path
    );
    Ok(())
}