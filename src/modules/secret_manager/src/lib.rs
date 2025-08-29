/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/lib.rs
*
* This file is the main entry point for the `secret_manager` dynamic library.
* It defines the FFI (Foreign Function Interface) boundary for the C core and
* orchestrates the secret management workflows.
*
* It exposes two main functions to the C layer:
* 1. `run_secret_sync`: Handles the synchronization of secrets from a provider
*    (like Vault) to a Kubernetes Secret. It deserializes the request, fetches
*    all requested secrets concurrently, and applies the resulting Kubernetes
*    `Secret` object to the cluster idempotently.
* 2. `run_secret_rotation`: Handles the rotation of a specific secret credential
*    at the provider level. It deserializes the request and calls the appropriate
*    provider's rotation logic.
*
* Both functions are designed to be robust, with comprehensive error handling
* and panic safety to ensure the stability of the calling C application.
*
* SPDX-License-Identifier: Apache-2.0 */

mod providers;
mod rotation_logic;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use futures::future::join_all;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams},
    Client,
};
use providers::{get_provider, ProviderConfig};
use rotation_logic::RotateRequest;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::{c_char, CStr};
use std::panic;

// --- FFI Configuration Structures ---

/// Defines a single secret to be fetched and its destination key in the K8s Secret.
#[derive(Deserialize, Debug)]
pub struct SecretSpec {
    /// The key name within the Kubernetes `Secret` object's `data` field.
    pub name: String,
    /// The provider-specific key or path to the source secret value.
    pub value_from: String,
}

/// The top-level configuration for a secret synchronization request.
#[derive(Deserialize, Debug)]
pub struct SyncRequest {
    /// The configuration for the secret provider backend (e.g., Vault, SOPS).
    pub provider: ProviderConfig,
    /// The Kubernetes namespace to sync the secret into.
    pub namespace: String,
    /// The name of the Kubernetes `Secret` object to create or update.
    pub secret_name: String,
    /// A list of secret keys and their sources.
    pub secrets: Vec<SecretSpec>,
}

// --- FFI Entry Points ---

/// The FFI entry point for the C core to run a secret synchronization.
///
/// # Safety
/// The `config_json` pointer must be a valid, null-terminated C string.
///
/// # Returns
/// - `0` on success.
/// - `-1` on a null pointer input.
/// - `-2` on a UTF-8 conversion error.
/// - `-3` on a JSON parsing error.
/// - `-4` on a runtime execution error.
/// - `-5` on a panic.
#[no_mangle]
pub extern "C" fn run_secret_sync(config_json: *const c_char) -> i32 {
    let result = panic::catch_unwind(|| {
        if config_json.is_null() {
            log::error!("[secret_manager] FFI Error: Received a null pointer for sync.");
            return -1;
        }
        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                log::error!("[secret_manager] FFI Error: Invalid UTF-8 in sync payload: {}", e);
                return -2;
            }
        };
        let request: SyncRequest = match serde_json::from_str(rust_str) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[secret_manager] FFI Error: Failed to parse JSON for sync: {}", e);
                return -3;
            }
        };

        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[secret_manager] Runtime Error: Failed to build Tokio runtime: {}", e);
                return -4;
            }
        };

        match runtime.block_on(run_sync_internal(request)) {
            Ok(_) => 0,
            Err(e) => {
                log::error!("[secret_manager] Execution Error during sync: {:?}", e);
                -4
            }
        }
    });

    result.unwrap_or(-5)
}

/// The FFI entry point for the C core to run a secret rotation.
///
/// # Safety
/// The `config_json` pointer must be a valid, null-terminated C string.
///
/// # Returns
/// - `0` on success.
/// - `-1` on a null pointer input.
/// - `-2` on a UTF-8 conversion error.
/// - `-3` on a JSON parsing error.
/// - `-4` on a runtime execution error.
/// - `-5` on a panic.
#[no_mangle]
pub extern "C" fn run_secret_rotation(config_json: *const c_char) -> i32 {
    let result = panic::catch_unwind(|| {
        if config_json.is_null() {
            log::error!("[secret_manager] FFI Error: Received a null pointer for rotation.");
            return -1;
        }
        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                log::error!("[secret_manager] FFI Error: Invalid UTF-8 in rotation payload: {}", e);
                return -2;
            }
        };
        let request: RotateRequest = match serde_json::from_str(rust_str) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[secret_manager] FFI Error: Failed to parse JSON for rotation: {}", e);
                return -3;
            }
        };

        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[secret_manager] Runtime Error: Failed to build Tokio runtime: {}", e);
                return -4;
            }
        };

        match runtime.block_on(rotation_logic::run_rotation_internal(request)) {
            Ok(_) => 0,
            Err(e) => {
                log::error!("[secret_manager] Execution Error during rotation: {:?}", e);
                -4
            }
        }
    });

    result.unwrap_or(-5)
}

// --- Core Orchestration Logic ---

/// The internal async function that contains the core orchestration logic for sync.
async fn run_sync_internal(request: SyncRequest) -> Result<()> {
    log::info!(
        "Starting secret sync for K8s Secret '{}' in namespace '{}'...",
        request.secret_name,
        request.namespace
    );

    // 1. Instantiate the configured secret provider using the factory.
    let provider = get_provider(request.provider)?;

    // 2. Fetch all secret values concurrently for maximum performance.
    let fetch_futures = request.secrets.iter().map(|spec| {
        let provider_ref = &provider;
        async move {
            let value = provider_ref.fetch_secret_value(&spec.value_from).await?;
            Ok((spec.name.clone(), value))
        }
    });

    let fetched_results: Vec<Result<(String, String)>> = join_all(fetch_futures).await;

    // 3. Collect results and build the Kubernetes Secret `data` map.
    let mut secret_data = BTreeMap::new();
    for result in fetched_results {
        let (name, value) = result.context("A failure occurred while fetching one of the secrets")?;
        secret_data.insert(name, kube::core::SecretData(B64.encode(value).into_bytes()));
    }

    // 4. Initialize Kubernetes client and prepare the Secret object.
    let client = Client::try_default().await.context("Failed to create Kubernetes client")?;
    let api: Api<Secret> = Api::namespaced(client, &request.namespace);

    let secret_manifest = Secret {
        metadata: ObjectMeta {
            name: Some(request.secret_name.clone()),
            namespace: Some(request.namespace.clone()),
            ..ObjectMeta::default()
        },
        data: Some(secret_data),
        ..Secret::default()
    };

    // 5. Apply the Secret to the cluster using Server-Side Apply.
    let ssapply = PatchParams::apply("ph.secret_manager");
    api.patch(&request.secret_name, &ssapply, &Patch::Apply(&secret_manifest))
        .await
        .with_context(|| format!("Failed to apply Kubernetes Secret '{}'", request.secret_name))?;

    log::info!("Successfully synchronized Secret '{}'.", request.secret_name);
    Ok(())
}