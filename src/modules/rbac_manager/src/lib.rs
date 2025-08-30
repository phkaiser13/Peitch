/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: src/modules/rbac_manager/src/lib.rs
*
* This module provides the core logic for managing Kubernetes RBAC resources.
* It is designed to be called from the C CLI handler and abstracts away the
* complexities of Kubernetes API interactions.
*
* SPDX-License-Identifier: Apache-2.0
*/

use anyhow::{anyhow, Context, Result};
use audit_logger::{log_audit_event, Target};
use k8s_openapi::api::rbac::v1 as rbac;
use kube::{
    api::{Api, ObjectMeta, PostParams},
    Client, Config,
};
use kube::config::{Kubeconfig, KubeConfigOptions};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::LazyLock;

// --- Role Mapping ---
// Maps user-friendly role names to actual Kubernetes ClusterRole names.
// These ClusterRoles are expected to be pre-installed on the cluster.
static ROLE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("promoter", "ph-cluster-promoter");
    m.insert("preview-admin", "ph-preview-administrator");
    m.insert("secrets-rotator", "ph-secrets-rotator");
    m
});


// --- Data Structures for JSON Deserialization ---

#[derive(Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
enum RBACOperation {
    Grant(GrantPayload),
    Revoke(RevokePayload),
}

#[derive(Deserialize, Debug)]
struct GrantPayload {
    role: String,
    subject: String,
    cluster: String,
}

#[derive(Deserialize, Debug)]
struct RevokePayload {
    role: String,
    subject: String,
    cluster: String,
}

// --- FFI Entry Point ---

/// # Safety
///
/// This function is unsafe because it dereferences a raw pointer `json_payload`.
/// The caller must ensure that this pointer is valid and points to a null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn run_rbac_manager(json_payload: *const c_char) -> i32 {
    // --- 1. Safely handle the C string ---
    if json_payload.is_null() {
        eprintln!("[rbac_manager] Error: Received null JSON payload.");
        return 1;
    }
    let payload_cstr = unsafe { CStr::from_ptr(json_payload) };
    let payload_str = match payload_cstr.to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[rbac_manager] Error: Invalid UTF-8 in JSON payload: {}", e);
            return 1;
        }
    };

    // --- 2. Deserialize the JSON payload ---
    let operation: RBACOperation = match serde_json::from_str(payload_str) {
        Ok(op) => op,
        Err(e) => {
            eprintln!("[rbac_manager] Error: Failed to deserialize JSON: {}", e);
            eprintln!("[rbac_manager] Payload: {}", payload_str);
            return 1;
        }
    };

    // --- 3. Execute the operation in an async runtime ---
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        match execute_rbac_operation(operation).await {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("[rbac_manager] Error: {}", e);
                1
            }
        }
    });

    result
}

// --- Core Logic ---

/// Creates a Kubernetes client for a specific cluster context.
async fn get_client_for_cluster(cluster_name: &str) -> Result<Client> {
    let mut kubeconfig_path = PathBuf::from("/etc/ph/kubeconfigs");
    kubeconfig_path.push(format!("{}.yaml", cluster_name));

    println!("[rbac_manager] DEBUG: Using kubeconfig path: {}", kubeconfig_path.display());

    if !kubeconfig_path.exists() {
        println!("[rbac_manager] WARN: Kubeconfig for cluster '{}' not found at {}. Falling back to default.", cluster_name, kubeconfig_path.display());
        let config = Config::infer().await
            .with_context(|| "Failed to infer default Kubernetes config.")?;
        return Client::try_from(config)
            .with_context(|| "Failed to create client from inferred config.");
    }

    let kubeconfig = Kubeconfig::read_from(&kubeconfig_path)
        .with_context(|| format!("Failed to read kubeconfig from '{}'", kubeconfig_path.display()))?;

    let options = KubeConfigOptions::default();

    let config = Config::from_custom_kubeconfig(kubeconfig, &options).await
        .with_context(|| "Failed to create config from custom kubeconfig")?;

    Client::try_from(config)
        .with_context(|| "Failed to create client from custom kubeconfig.")
}


async fn execute_rbac_operation(operation: RBACOperation) -> Result<()> {
    match operation {
        RBACOperation::Grant(payload) => {
            let client = get_client_for_cluster(&payload.cluster).await?;
            handle_grant(client, payload).await?;
        }
        RBACOperation::Revoke(payload) => {
            let client = get_client_for_cluster(&payload.cluster).await?;
            handle_revoke(client, payload).await?;
        }
    }
    Ok(())
}

fn get_subject(subject_str: &str) -> (String, String) {
    if subject_str.starts_with("user:") {
        ("User".to_string(), subject_str.strip_prefix("user:").unwrap().to_string())
    } else if subject_str.starts_with("group:") {
        ("Group".to_string(), subject_str.strip_prefix("group:").unwrap().to_string())
    } else {
        // Default to user if no prefix is provided
        ("User".to_string(), subject_str.to_string())
    }
}

// The build_role_binding and get_binding_name functions are no longer needed here.
// This logic has been moved to the rbac_policy_controller.


use ph_operator::controllers::rbac_policy_controller::{PhgitRbacPolicy, PhgitRbacPolicySpec, Subject as RbacSubject};

async fn handle_grant(client: Client, payload: GrantPayload) -> Result<()> {
    println!(
        "[rbac_manager] Creating PhgitRbacPolicy to grant role '{}' to subject '{}' on cluster '{}'.",
        payload.role, payload.subject, payload.cluster
    );

    let (subject_kind, subject_name) = get_subject(&payload.subject);
    
    // The policy name should be deterministic and safe for a resource name.
    let policy_name = format!("ph-policy-{}-{}", payload.role, subject_name.replace(['@', '.'], "-"));

    let policy_spec = PhgitRbacPolicySpec {
        role: payload.role.clone(),
        subject: RbacSubject {
            kind: subject_kind,
            name: subject_name,
        },
    };

    let policy = PhgitRbacPolicy::new(&policy_name, policy_spec);
    
    // Policies are created in the 'phgit-rbac' namespace by convention.
    let api: Api<PhgitRbacPolicy> = Api::namespaced(client.clone(), "phgit-rbac");
    
    api.create(&PostParams::default(), &policy)
        .await
        .with_context(|| format!("Failed to create PhgitRbacPolicy '{}'", policy_name))?;

    println!(
        "[rbac_manager] Successfully created PhgitRbacPolicy '{}' in namespace 'phgit-rbac'. The controller will now create the RoleBinding.",
        policy_name
    );

    // --- Audit Logging ---
    let mut details = BTreeMap::new();
    details.insert("role".to_string(), payload.role.clone());
    details.insert("subject".to_string(), payload.subject.clone());

    let target = Some(Target {
        kind: Some("Cluster".to_string()),
        name: Some(payload.cluster.clone()),
        namespace: None,
    });

    // The actor is unknown in this context, so we pass None.
    if let Err(e) = log_audit_event(
        client,
        "grant".to_string(),
        "rbac_manager".to_string(),
        None,
        target,
        details,
    )
    .await
    {
        eprintln!("[rbac_manager] WARNING: Failed to create audit event: {}", e);
    }

    Ok(())
}

async fn handle_revoke(client: Client, payload: RevokePayload) -> Result<()> {
    println!(
        "[rbac_manager] Deleting PhgitRbacPolicy to revoke role '{}' from subject '{}' on cluster '{}'.",
        payload.role, payload.subject, payload.cluster
    );

    let (_, subject_name) = get_subject(&payload.subject);
    let policy_name = format!("ph-policy-{}-{}", payload.role, subject_name.replace(['@', '.'], "-"));

    let api: Api<PhgitRbacPolicy> = Api::namespaced(client.clone(), "phgit-rbac");

    let result = match api.delete(&policy_name, &Default::default()).await {
        Ok(_) => {
            println!(
                "[rbac_manager] Successfully deleted PhgitRbacPolicy '{}'. The controller will now remove the RoleBinding.",
                policy_name
            );
            Ok(())
        }
        Err(kube::Error::Api(e)) if e.code == 404 => {
            println!(
                "[rbac_manager] PhgitRbacPolicy '{}' not found. Assuming already revoked.",
                policy_name
            );
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("Failed to delete PhgitRbacPolicy '{}'", policy_name)),
    };

    if result.is_ok() {
        // --- Audit Logging ---
        let mut details = BTreeMap::new();
        details.insert("role".to_string(), payload.role.clone());
        details.insert("subject".to_string(), payload.subject.clone());

        let target = Some(Target {
            kind: Some("Cluster".to_string()),
            name: Some(payload.cluster.clone()),
            namespace: None,
        });

        if let Err(e) = log_audit_event(
            client,
            "revoke".to_string(),
            "rbac_manager".to_string(),
            None,
            target,
            details,
        )
        .await
        {
            eprintln!("[rbac_manager] WARNING: Failed to create audit event: {}", e);
        }
    }

    result
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    // The tests for build_role_binding and get_binding_name have been removed as the functions
    // are no longer part of this module. The test for handle_grant would need to be rewritten
    // to mock the creation of PhgitRbacPolicy resources instead of RoleBindings.
    // For now, we are only keeping the tests for the remaining helper functions.

    #[test]
    fn test_get_subject_parsing() {
        let (kind, name) = get_subject("user:test@example.com");
        assert_eq!(kind, "User");
        assert_eq!(name, "test@example.com");

        let (kind, name) = get_subject("group:app-admins");
        assert_eq!(kind, "Group");
        assert_eq!(name, "app-admins");

        let (kind, name) = get_subject("justauser");
        assert_eq!(kind, "User");
        assert_eq!(name, "justauser");
    }
}
