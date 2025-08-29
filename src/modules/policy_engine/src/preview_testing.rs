/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/policy_engine/src/preview_testing.rs
*
* This module implements the logic for the 'policy test' subcommand. Its
* responsibility is to test the Rego policies against the resources that are
* currently running in an ephemeral Kubernetes preview environment.
*
* The workflow is as follows:
* 1. Constructs the target namespace name based on the provided Pull Request
*    number (e.g., `pr-123`).
* 2. Connects to the Kubernetes cluster using the `kube` client.
* 3. Uses the Kubernetes Discovery API to find all resource types
*    that are `namespaced` and can be listed. This makes the solution
*    robust and adaptable to custom CRDs without the need to hardcode
*    specific resource types.
* 4. Lists all instances of these resources in the target namespace.
* 5. Serializes each found resource to YAML format.
* 6. Creates a temporary directory and saves all the YAMLs in it.
* 7. Invokes the `conftest` tool as a subprocess, pointing to the
*    policy directory and the temporary directory with the resource YAMLs.
* 8. Parses the exit code and output of `conftest` to determine if
*    the tests passed or failed, returning an appropriate result.
*
* SPDX-License-Identifier: Apache-2.0 */

use anyhow::{anyhow, Context, Result};
use kube::{
    api::{Api, DynamicObject, ListParams},
    discovery::{self, Scope},
    Client,
};
use std::{fs, str};
use tempfile::Builder;
use tokio::process::Command;

/// Main entry point for the 'test' logic.
/// Connects to the cluster, fetches resources from the PR namespace, and tests them with conftest.
pub async fn test_preview_policies(policy_repo_path: &str, pr_number: u32) -> Result<()> {
    println!("[RUST PREVIEW] Starting policy test for PR #{}", pr_number);
    let namespace = format!("pr-{}", pr_number);

    // 1. Connect to the Cluster
    println!("[RUST PREVIEW] Initializing Kubernetes client...");
    let client = Client::try_default()
        .await
        .context("Failed to create Kubernetes client from default kubeconfig.")?;
    println!("[RUST PREVIEW] Kubernetes client initialized successfully.");

    // 2. Create a temporary directory to store the resource YAMLs.
    // The directory will be automatically cleaned up when `temp_dir` goes out of scope.
    let temp_dir = Builder::new()
        .prefix(&format!("ph-preview-test-{}-", pr_number))
        .tempdir()
        .context("Failed to create temporary directory for manifests.")?;
    let temp_path = temp_dir.path();
    println!("[RUST PREVIEW] Temporary directory created at: {:?}", temp_path);

    // 3. List Resources
    println!("[RUST PREVIEW] Fetching resources in namespace '{}'...", namespace);
    let discovery = discovery::Discovery::new(client.clone()).run().await?;
    let mut resource_count = 0;

    // Iterate over all discovered API groups (core, apps, etc.)
    for group in discovery.groups() {
        for (ar, caps) in group.resources() {
            // We are interested in resources that live within a namespace and that we can list.
            if caps.scope == Scope::Namespaced && caps.verbs.contains("list") {
                let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), &namespace, &ar);
                match api.list(&ListParams::default()).await {
                    Ok(list) => {
                        if list.items.is_empty() {
                            continue;
                        }
                        println!("[RUST PREVIEW]   -> Found {} resource(s) of type {}", list.items.len(), ar.kind);
                        for item in list.items {
                            // Clean up runtime metadata to make the test cleaner and focused on the specification.
                            let mut clean_item = item.clone();
                            clean_item.metadata.managed_fields = None;
                            clean_item.metadata.resource_version = None;
                            clean_item.metadata.uid = None;
                            clean_item.metadata.creation_timestamp = None;
                            clean_item.metadata.generation = None;

                            let yaml_content = serde_yaml::to_string(&clean_item)
                                .context(format!("Failed to serialize resource '{}' to YAML.", clean_item.metadata.name.as_deref().unwrap_or("unknown")))?;

                            let file_name = format!(
                                "{}_{}.yaml",
                                ar.kind.to_lowercase(),
                                clean_item.metadata.name.as_deref().unwrap_or("unnamed")
                            );
                            let file_path = temp_path.join(file_name);
                            fs::write(&file_path, yaml_content).context(format!("Failed to write temporary YAML to {:?}", file_path))?;
                            resource_count += 1;
                        }
                    }
                    Err(e) => {
                        // Print a warning, but do not fail. It could be a permission issue (RBAC).
                        println!("[RUST PREVIEW]   -> Warning: Could not list resources of type {}: {}", ar.kind, e);
                    }
                }
            }
        }
    }

    if resource_count == 0 {
        println!("[RUST PREVIEW] No resources found in namespace '{}'. Test completed successfully.", namespace);
        return Ok(());
    }
    println!("[RUST PREVIEW] Total of {} resources saved for analysis.", resource_count);

    // 4. Execute conftest
    println!("[RUST PREVIEW] Running 'conftest' against the extracted resources...");
    let output = Command::new("conftest")
        .arg("test")
        .arg("--policy")
        .arg(policy_repo_path)
        .arg(temp_path.to_str().unwrap()) // Pass the directory with the YAMLs
        .output()
        .await
        .context("Failed to execute 'conftest' process. Check if it is installed and in the system's PATH.")?;

    // 5. Analyze the Results
    let stdout = str::from_utf8(&output.stdout)?;
    let stderr = str::from_utf8(&output.stderr)?;

    if output.status.success() {
        println!("[RUST PREVIEW] ✅ Policy tests passed successfully!");
        if !stdout.trim().is_empty() {
            println!("--- Conftest Output ---\n{}\n-----------------------", stdout);
        }
        Ok(())
    } else {
        // If conftest fails, its output is the best explanation of what went wrong.
        let full_output = format!(
            "--- Standard Output (stdout) ---\n{}\n--- Standard Error (stderr) ---\n{}",
            stdout, stderr
        );
        Err(anyhow!(
            "❌ Policy violations detected by conftest.\n\n{}",
            full_output
        ))
    }
}