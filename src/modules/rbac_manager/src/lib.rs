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
use kube::{
    api::{Api, ObjectMeta, PostParams},
    Client, Config,
};
use kube::config::{Kubeconfig, KubeConfigOptions};
use k8s_openapi::api::rbac::v1 as rbac;
use serde::Deserialize;
use std::collections::HashMap;
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

// --- Business Logic Helpers for Testability ---

/// Constructs a RoleBinding object from a grant payload.
fn build_role_binding(payload: &GrantPayload) -> Result<rbac::RoleBinding> {
    let k8s_role_name = ROLE_MAP.get(payload.role.as_str())
        .ok_or_else(|| anyhow!("Role '{}' is not a valid, predefined role.", payload.role))?;

    let (subject_kind, subject_name) = get_subject(&payload.subject);

    let binding_name = get_binding_name(&payload.role, &subject_kind, &subject_name);

    let binding = rbac::RoleBinding {
        metadata: ObjectMeta {
            name: Some(binding_name.clone()),
            namespace: Some("default".to_string()),
            ..Default::default()
        },
        role_ref: rbac::RoleRef {
            api_group: "rbac.authorization.k8s.io".to_string(),
            kind: "ClusterRole".to_string(),
            name: k8s_role_name.to_string(),
        },
        subjects: Some(vec![rbac::Subject {
            kind: subject_kind,
            name: subject_name,
            api_group: Some("rbac.authorization.k8s.io".to_string()),
            namespace: None,
        }]),
    };
    Ok(binding)
}

/// Generates the deterministic name for a RoleBinding.
fn get_binding_name(role: &str, subject_kind: &str, subject_name: &str) -> String {
    format!("ph-{}-{}-{}", role, subject_kind.to_lowercase(), subject_name.replace(['@', '.'], "-"))
}


async fn handle_grant(client: Client, payload: GrantPayload) -> Result<()> {
    println!(
        "[rbac_manager] Granting role '{}' to subject '{}' on cluster '{}'.",
        payload.role, payload.subject, payload.cluster
    );

    let binding = build_role_binding(&payload)?;
    let binding_name = binding.metadata.name.clone().unwrap_or_default();

    let api: Api<rbac::RoleBinding> = Api::namespaced(client, "default");
    let params = PostParams::default();

    api.create(&params, &binding).await
        .with_context(|| format!("Failed to create RoleBinding '{}'", binding_name))?;

    println!("[rbac_manager] Successfully created RoleBinding '{}' in namespace 'default'.", binding_name);
    Ok(())
}

async fn handle_revoke(client: Client, payload: RevokePayload) -> Result<()> {
    println!(
        "[rbac_manager] Revoking role '{}' from subject '{}' on cluster '{}'.",
        payload.role, payload.subject, payload.cluster
    );

    let (subject_kind, subject_name) = get_subject(&payload.subject);
    let binding_name = get_binding_name(&payload.role, &subject_kind, &subject_name);


    let api: Api<rbac::RoleBinding> = Api::namespaced(client, "default");

    match api.delete(&binding_name, &Default::default()).await {
        Ok(_) => {
            println!("[rbac_manager] Successfully deleted RoleBinding '{}' from namespace 'default'.", binding_name);
            Ok(())
        },
        Err(kube::Error::Api(e)) if e.code == 404 => {
            println!("[rbac_manager] RoleBinding '{}' not found. Assuming already revoked.", binding_name);
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("Failed to delete RoleBinding '{}'", binding_name)),
    }
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_get_binding_name_generation() {
        let name = get_binding_name("promoter", "User", "dev@corp.com");
        assert_eq!(name, "ph-promoter-user-dev-corp-com");

        let name = get_binding_name("preview-admin", "Group", "preview-team");
        assert_eq!(name, "ph-preview-admin-group-preview-team");
    }

    #[test]
    fn test_build_role_binding_success() {
        let payload = GrantPayload {
            role: "promoter".to_string(),
            subject: "user:dev@corp.com".to_string(),
            cluster: "test-cluster".to_string(),
        };

        let binding = build_role_binding(&payload).unwrap();
        
        assert_eq!(binding.metadata.name.as_deref(), Some("ph-promoter-user-dev-corp-com"));
        assert_eq!(binding.metadata.namespace.as_deref(), Some("default"));

        let role_ref = binding.role_ref;
        assert_eq!(role_ref.kind, "ClusterRole");
        assert_eq!(role_ref.name, "ph-cluster-promoter");
        assert_eq!(role_ref.api_group, "rbac.authorization.k8s.io");

        let subjects = binding.subjects.unwrap();
        assert_eq!(subjects.len(), 1);
        let subject = &subjects[0];
        assert_eq!(subject.kind, "User");
        assert_eq!(subject.name, "dev@corp.com");
        assert_eq!(subject.api_group.as_deref(), Some("rbac.authorization.k8s.io"));
    }

    #[test]
    fn test_build_role_binding_invalid_role() {
        let payload = GrantPayload {
            role: "non-existent-role".to_string(),
            subject: "user:hacker@bad.net".to_string(),
            cluster: "test-cluster".to_string(),
        };

        let result = build_role_binding(&payload);
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert_eq!(error.to_string(), "Role 'non-existent-role' is not a valid, predefined role.");
    }
}
