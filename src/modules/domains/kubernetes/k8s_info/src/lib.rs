/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_info/src/lib.rs
 *
 * This module provides a Foreign Function Interface (FFI) for C code to
 * interact with Kubernetes clusters. It exposes the `run_k8s_info` function
 * which accepts a JSON payload containing cluster information and returns
 * formatted cluster details to stdout.
 *
 * The module uses the kube-rs library to interact with the Kubernetes API
 * and automatically discovers the cluster configuration from the local
 * kubeconfig file. It provides essential cluster information including
 * server version, node status, and basic cluster health metrics.
 *
 * SPDX-License-Identifier: Apache-2.0 */

use kube::{Client, api::{Api, ListParams}};
use k8s_openapi::api::core::v1::Node;
use serde_json::Value;
use std::ffi::{CStr, CString};
use libc::c_char;
use anyhow::{Result, Context};

/// FFI function called from C code to retrieve and display Kubernetes cluster information
///
/// # Arguments
/// * `payload_json` - A C string pointer containing JSON with cluster information
///
/// # Returns
/// * `0` on success
/// * Negative values on various error conditions:
///   * `-1`: Invalid payload or encoding error
///   * `-2`: JSON parsing error
///   * `-3`: Kubernetes API error
///   * `-4`: Runtime initialization error
#[no_mangle]
pub extern "C" fn run_k8s_info(payload_json: *const c_char) -> i32 {
    // Convert C pointer to Rust string
    let c_str = unsafe { CStr::from_ptr(payload_json) };
    let payload_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Error: Invalid JSON payload or encoding issue.");
            return -1;
        }
    };

    // Parse the JSON payload
    let params: Value = match serde_json::from_str(payload_str) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: Could not parse the JSON payload: {}", e);
            return -2;
        }
    };

    let cluster_name = params["cluster"].as_str().unwrap_or("unknown");
    println!("🔍 Getting cluster information: '{}'...", cluster_name);

    // Initialize Tokio runtime for async operations
    let rt = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Error: Failed to initialize async runtime: {}", e);
            return -4;
        }
    };

    // Execute the main async function
    rt.block_on(async {
        match fetch_and_print_cluster_info(cluster_name).await {
            Ok(_) => {
                println!("\n✅ Cluster information successfully retrieved!");
                0
            },
            Err(e) => {
                eprintln!("❌ Failed to get Kubernetes information: {}", e);
                -3
            }
        }
    })
}

/// Main async function that retrieves and displays comprehensive cluster information
///
/// # Arguments
/// * `cluster_name` - The name of the cluster for display purposes
///
/// # Returns
/// * `Result<(), anyhow::Error>` - Success or detailed error information
async fn fetch_and_print_cluster_info(cluster_name: &str) -> Result<()> {
    // Initialize Kubernetes client using default kubeconfig
    let client = Client::try_default()
        .await
        .context("Failed to connect to the Kubernetes cluster. Check if kubeconfig is configured correctly.")?;

    println!("\n📋 Cluster Summary: {}", cluster_name);
    println!("{'=':<50}");

    // 1. Get and display server version
    match get_server_version(&client).await {
        Ok(_) => {},
        Err(e) => eprintln!("⚠️  Warning: Could not get server version: {}", e),
    }

    // 2. Get and display node information
    match get_node_information(&client).await {
        Ok(_) => {},
        Err(e) => eprintln!("⚠️  Warning: Could not get node information: {}", e),
    }

    // 3. Get and display namespace count
    match get_namespace_count(&client).await {
        Ok(_) => {},
        Err(e) => eprintln!("⚠️  Warning: Could not count namespaces: {}", e),
    }

    // 4. Get and display basic resource counts
    match get_basic_resource_counts(&client).await {
        Ok(_) => {},
        Err(e) => eprintln!("⚠️  Warning: Could not get resource counts: {}", e),
    }

    Ok(())
}

/// Retrieves and displays Kubernetes server version information
async fn get_server_version(client: &Client) -> Result<()> {
    let version = client.apiserver_version().await
        .context("Failed to get server version")?;
    
    println!("\n🚀 Kubernetes Version");
    println!("   Server: v{}.{}", version.major, version.minor);
    if let Some(git_version) = &version.git_version {
        println!("   Git Version: {}", git_version);
    }
    
    Ok(())
}

/// Retrieves and displays detailed node information
async fn get_node_information(client: &Client) -> Result<()> {
    let nodes: Api<Node> = Api::all(client.clone());
    let node_list = nodes.list(&ListParams::default()).await
        .context("Failed to list cluster nodes")?;

    println!("\n🖥️  Cluster Nodes");
    println!("   Total nodes: {}", node_list.items.len());
    
    if node_list.items.is_empty() {
        println!("   ⚠️  No nodes found in the cluster");
        return Ok(());
    }

    let mut ready_nodes = 0;
    let mut not_ready_nodes = 0;

    for node in &node_list.items {
        let node_name = node.metadata.name.as_ref().unwrap_or(&"<no-name>".to_string());
        
        // Check node readiness
        let ready_status = node.status.as_ref()
            .and_then(|status| status.conditions.as_ref())
            .and_then(|conditions| {
                conditions.iter().find(|c| c.type_ == "Ready")
            })
            .map(|condition| condition.status == "True")
            .unwrap_or(false);

        let status_icon = if ready_status { "✅" } else { "❌" };
        let status_text = if ready_status { "Ready" } else { "NotReady" };
        
        if ready_status {
            ready_nodes += 1;
        } else {
            not_ready_nodes += 1;
        }

        // Get node role if available
        let role = node.metadata.labels.as_ref()
            .and_then(|labels| {
                if labels.contains_key("node-role.kubernetes.io/control-plane") ||
                   labels.contains_key("node-role.kubernetes.io/master") {
                    Some("control-plane")
                } else if labels.contains_key("node-role.kubernetes.io/worker") {
                    Some("worker")
                } else {
                    Some("worker") // Assume worker if no specific role found
                }
            })
            .unwrap_or("unknown");

        println!("   {} {} ({}) - Role: {}", status_icon, node_name, status_text, role);
    }

    // Summary
    println!("\n   📊 Node Summary:");
    println!("   • Ready: {}", ready_nodes);
    if not_ready_nodes > 0 {
        println!("   • Not Ready: {}", not_ready_nodes);
    }

    Ok(())
}

/// Retrieves and displays namespace count
async fn get_namespace_count(client: &Client) -> Result<()> {
    let namespaces: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(client.clone());
    let namespace_list = namespaces.list(&ListParams::default()).await
        .context("Failed to list namespaces")?;

    println!("\n📦 Namespaces");
    println!("   Total: {}", namespace_list.items.len());

    // Show some common namespaces if they exist
    let common_namespaces = ["default", "kube-system", "kube-public", "kube-node-lease"];
    let mut found_common = Vec::new();
    
    for ns in &namespace_list.items {
        if let Some(name) = &ns.metadata.name {
            if common_namespaces.contains(&name.as_str()) {
                found_common.push(name.clone());
            }
        }
    }

    if !found_common.is_empty() {
        println!("   System namespaces: {}", found_common.join(", "));
    }

    Ok(())
}

/// Retrieves and displays basic resource counts
async fn get_basic_resource_counts(client: &Client) -> Result<()> {
    println!("\n📈 Basic Resources");

    // Count pods
    let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::all(client.clone());
    match pods.list(&ListParams::default()).await {
        Ok(pod_list) => {
            let running_pods = pod_list.items.iter()
                .filter(|pod| {
                    pod.status.as_ref()
                        .and_then(|status| status.phase.as_ref())
                        .map(|phase| phase == "Running")
                        .unwrap_or(false)
                })
                .count();
            
            println!("   Pods: {} total ({} running)", pod_list.items.len(), running_pods);
        },
        Err(_) => println!("   Pods: Could not retrieve information"),
    }

    // Count services
    let services: Api<k8s_openapi::api::core::v1::Service> = Api::all(client.clone());
    match services.list(&ListParams::default()).await {
        Ok(service_list) => {
            println!("   Services: {}", service_list.items.len());
        },
        Err(_) => println!("   Services: Could not retrieve information"),
    }

    // Count deployments
    let deployments: Api<k8s_openapi::api::apps::v1::Deployment> = Api::all(client.clone());
    match deployments.list(&ListParams::default()).await {
        Ok(deployment_list) => {
            println!("   Deployments: {}", deployment_list.items.len());
        },
        Err(_) => println!("   Deployments: Could not retrieve information"),
    }

    Ok(())
}