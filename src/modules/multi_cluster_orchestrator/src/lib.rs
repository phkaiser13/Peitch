/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/multi_cluster_orchestrator/src/lib.rs
*
* This file is the main entry point for the `multi_cluster_orchestrator` dynamic
* library. It defines the FFI boundary that allows the C core to invoke the
* Rust logic. Its primary responsibility is to safely handle data from C,
* deserialize the JSON payload into strongly-typed Rust structs, set up the
* asynchronous Tokio runtime, and orchestrate the module's business logic via
* the `ClusterManager`.
*
* This version has been updated to handle a more complex `Action` structure.
* It now deserializes a `strategy` object within the `apply` action and passes
* it explicitly to the `ClusterManager`, enabling advanced deployment patterns
* like "staged" and "failover".
*
* SPDX-License-Identifier: Apache-2.0 */

mod cluster_manager;
mod dr_crd;

// --- Generated Protobuf Structs ---
// This includes the generated code from `rpc_data.proto`, including our `ErrorPayload`.
pub mod ph {
    pub mod ipc {
        include!(concat!(env!("OUT_DIR"), "/ph.ipc.rs"));
    }
}

use crate::cluster_manager::{Cluster, ClusterManager, ClustersConfig, MultiClusterConfig};
use crate::dr_crd::{
    ClusterConnection, DrClusterConnection, DrPolicy, FailoverTrigger, HealthCheckPolicy,
    PhgitDisasterRecovery, PhgitDisasterRecoverySpec, TargetApplication,
};
use anyhow::{anyhow, Context, Result};
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    api::{Api, Patch, PatchParams, PostParams},
    Client, Config,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::ffi::{c_char, CStr};
use std::panic;
use thiserror::Error;
use prost::Message;

// --- Error Handling ---

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid FFI input: {0}")]
    FfiInputError(String),
    #[error("Failed to parse JSON configuration: {0}")]
    JsonParseFailed(#[from] serde_json::Error),
    #[error("Failed to build Tokio runtime: {0}")]
    RuntimeBuildFailed(#[from] std::io::Error),
    #[error("Cluster '{0}' not found in configuration.")]
    ClusterNotFound(String),
    #[error("Failed to initialize client for cluster '{cluster_name}': {source}")]
    ClientInitialization {
        cluster_name: String,
        source: anyhow::Error,
    },
    #[error("Failed to execute multi-cluster action: {0}")]
    ExecutionFailed(#[from] anyhow::Error),
}


// --- FFI Payload Deserialization ---

/// Writes a structured error into the buffer provided by the C caller.
fn write_error(e: &Error, buf: *mut c_char, len: usize) {
    let payload = ph::ipc::ErrorPayload {
        code: format!("{:?}", e),
        message: e.to_string(),
        details: match e.source() {
            Some(source) => format!("{:#}", source),
            None => "".to_string(),
        },
    };

    // We serialize to JSON here because it's simpler for the C side to parse
    // than a raw protobuf binary message.
    let json_string = serde_json::to_string(&payload).unwrap_or_else(|_| "{\"code\":\"SerializationFailed\",\"message\":\"Could not serialize error payload.\",\"details\":\"\"}".to_string());
    
    if let Ok(c_string) = CString::new(json_string) {
        let bytes = c_string.as_bytes_with_nul();
        if bytes.len() <= len {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, bytes.len());
            }
        } else {
            // Buffer too small, write a truncated message
            let truncated_msg = "{\"code\":\"BufferTooSmall\"}";
            let bytes = CString::new(truncated_msg).unwrap().as_bytes_with_nul();
             unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, bytes.len().min(len));
            }
        }
    }
}

/// This enum represents the different high-level commands this module can execute.
/// It's designed to be deserialized from the initial JSON payload.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FailoverConfig {
    app: String,
    from_cluster: String,
    to_cluster: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
enum FfiAction {
    /// The original 'apply' functionality, now nested.
    Apply {
        #[serde(flatten)]
        config: MultiClusterConfig,
    },
    /// The new 'set_policy' functionality.
    SetPolicy {
        cluster_name: String,
        policy_file_path: String,
    },
    /// The new 'failover' functionality.
    Failover(FailoverConfig),
}

/// The main FFI entry point for the C core to run multi-cluster orchestration.
///
/// This function takes a JSON configuration specifying an action to perform.
///
/// # Safety
/// The `config_json` pointer must be a valid, null-terminated C string.
///
/// # Returns
/// - `0` on success.
/// - `-1` on a null pointer input.
/// - `-2` on a UTF-8 conversion error.
/// - `-3` on a JSON parsing error.
/// - `-4` on a runtime or initialization error.
/// - `-5` on a panic within the Rust code.
/// - `-6` if one or more cluster operations failed during execution.
#[no_mangle]
pub extern "C" fn run_multi_cluster_orchestrator(
    config_json: *const c_char,
    error_buf: *mut c_char,
    error_buf_len: usize,
) -> i32 {
    let result = panic::catch_unwind(|| {
        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                write_error(&Error::RuntimeBuildFailed(e), error_buf, error_buf_len);
                return -4;
            }
        };

        match runtime.block_on(run(config_json)) {
            Ok(_) => 0,
            Err(e) => {
                write_error(&e, error_buf, error_buf_len);
                // Return a generic error code; C side will parse the buffer for details.
                -1
            }
        }
    });

    result.unwrap_or({
        let e = Error::FfiInputError("Panic occurred in Rust module".to_string());
        write_error(&e, error_buf, error_buf_len);
        -5
    })
}

/// Main logic function, allows for `?` error propagation.
async fn run(config_json: *const c_char) -> Result<(), Error> {
    if config_json.is_null() {
        return Err(Error::FfiInputError("Received null pointer".to_string()));
    }
    let c_str = unsafe { CStr::from_ptr(config_json) };
    let rust_str = c_str.to_str().map_err(|e| Error::FfiInputError(e.to_string()))?;

    let ffi_action: FfiAction = serde_json::from_str(rust_str)?;

    match ffi_action {
        FfiAction::Apply { config } => handle_apply_action(config).await?,
        FfiAction::SetPolicy { cluster_name, policy_file_path } => {
            handle_set_policy_action(&cluster_name, &policy_file_path).await?
        }
        FfiAction::Failover(config) => handle_failover_action(config).await?,
    }

    Ok(())
}

/// Handles the logic for the 'failover' action by creating a PhgitDisasterRecovery resource.
async fn handle_failover_action(config: FailoverConfig) -> Result<(), Error> {
    println!(
        "Triggering Disaster Recovery for app '{}' from cluster '{}' to cluster '{}'",
        config.app, config.from_cluster, config.to_cluster
    );

    // This helper function remains local as it's only used here now.
    async fn get_client(cluster_name: &str) -> Result<Client, Error> {
        let kubeconfig_path = format!("/etc/ph/kubeconfigs/{}.yaml", cluster_name);
        let config = Config::from_kubeconfig(&kube::config::Kubeconfig::read_from(&kubeconfig_path).map_err(|e| anyhow!(e))?)
            .await
            .map_err(|e| anyhow!(e))?;
        Client::try_from(config).map_err(|e| anyhow!(e))
            .map_err(|source| Error::ClientInitialization{ cluster_name: cluster_name.to_string(), source })
    }

    // Assume the DR resource is created in the primary cluster's management namespace.
    let client = get_client(&config.from_cluster).await?;
    let api: Api<PhgitDisasterRecovery> = Api::default_namespaced(client);

    // The spec for the DR resource is constructed from the CLI input and defaults.
    // A real-world implementation might fetch a more detailed policy from a config source.
    let dr_spec = PhgitDisasterRecoverySpec {
        primary_cluster: ClusterConnection {
            kubeconfig_secret_ref: format!("{}-kubeconfig", config.from_cluster),
        },
        dr_cluster: DrClusterConnection {
            kubeconfig_secret_ref: format!("{}-kubeconfig", config.to_cluster),
            replicas: 3, // A reasonable default
        },
        target_application: TargetApplication {
            deployment_name: config.app.clone(),
            namespace: "default".to_string(), // Assuming default namespace for now
        },
        policy: DrPolicy {
            health_check: HealthCheckPolicy {
                // Placeholder policy details.
                prometheus_query: "vector(1)".to_string(),
                success_condition: "value == 1".to_string(),
                interval: "1m".to_string(),
                failure_threshold: 3,
            },
            // The CLI command is a manual trigger for the failover process.
            failover_trigger: FailoverTrigger::Manual,
            notification: Default::default(),
        },
    };

    // The name of the DR resource should be unique and descriptive, e.g., based on the app name.
    let dr_resource = PhgitDisasterRecovery::new(&config.app, dr_spec);

    // Create the resource in the cluster. This triggers the dr-controller.
    api.create(&PostParams::default(), &dr_resource)
        .await
        .map_err(|e| anyhow!(e))?;

    println!(
        "[SUCCESS] PhgitDisasterRecovery resource for app '{}' created. The DR operator will now handle the failover process.",
        config.app
    );

    Ok(())
}

/// Handles the logic for the 'apply' action.
async fn handle_apply_action(config: MultiClusterConfig) -> Result<(), Error> {
    let all_clusters_content = tokio::fs::read_to_string("config/clusters.yaml").await?;
    let all_clusters_config: ClustersConfig = serde_yaml::from_str(&all_clusters_content)
        .map_err(|e| anyhow::anyhow!(e))?;
    
    let target_names: HashSet<String> = config.targets.iter().map(|t| t.name.clone()).collect();
    let target_clusters: Vec<Cluster> = all_clusters_config.clusters.into_iter()
        .filter(|c| target_names.contains(&c.name))
        .collect();

    let manager = ClusterManager::new(&config.cluster_configs).await?;
    let results = manager.execute_action(&target_clusters, &config.action).await?;
    
    println!("\n--- Multi-Cluster Operation Report ---");
    let mut all_successful = true;
    for res in &results {
        match &res.outcome {
            Ok(msg) => {
                println!("[SUCCESS] Cluster: {}", res.cluster_name);
                println!("          Details: {:?}", msg);
            }
            Err(e) => {
                all_successful = false;
                println!("[FAILURE] Cluster: {}", res.cluster_name);
                println!("          Error: {}", e);
            }
        }
    }
    println!("--- End of Report ---");

    if all_successful {
        println!("\nAll operations completed successfully.");
        Ok(())
    } else {
        Err(Error::ExecutionFailed(anyhow!("One or more cluster operations failed.")))
    }
}

/// Handles the logic for the 'set_policy' action.
async fn handle_set_policy_action(cluster_name: &str, policy_file_path: &str) -> Result<(), Error> {
    // To call the modified `set_cluster_policy` method, we first need an instance
    // of the ClusterManager. To create one, we need to provide it with the
    // configuration for the cluster we intend to contact.
    
    // We'll build a map of cluster configurations containing only our target cluster.
    // The path to the kubeconfig is derived from a convention established elsewhere
    // in the application (see the 'multi' command in the C layer).
    let mut cluster_configs = BTreeMap::new();
    cluster_configs.insert(
        cluster_name.to_string(),
        format!("/etc/ph/kubeconfigs/{}.yaml", cluster_name),
    );

    // Create a new ClusterManager instance with the configuration for our target cluster.
    let manager = ClusterManager::new(&cluster_configs)
        .await
        .map_err(|e| Error::ExecutionFailed(e))?;

    // Now, call the instance method to apply the policy.
    manager
        .set_cluster_policy(cluster_name, policy_file_path)
        .await
        .map_err(|e| Error::ExecutionFailed(e))?;

    Ok(())
}