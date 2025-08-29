/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/audit_logger/src/lib.rs
 *
 * This module provides a reusable interface for creating PhgitAudit custom
 * resources. It centralizes the logic for generating audit events, ensuring
 * that all events have a consistent structure and are stored in the
 * Kubernetes API server, which acts as the centralized audit store.
 *
 * Architecture:
 * - Defines the Rust data structures that correspond to the PhgitAudit CRD.
 *   This provides a type-safe way to interact with the audit resources.
 * - Exposes a single public function, `log_audit_event`, which can be called
 *   by other Rust modules (like `rbac_manager`) to record significant events.
 * - The function is responsible for constructing the `PhgitAudit` object and
 *   using the `kube-rs` library to create it in the cluster.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::Result;
use chrono::{DateTime, Utc};
use kube::{
    api::{Api, PostParams},
    Client, CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// --- Custom Resource Definition Structs ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitAudit"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitAuditSpec {
    #[serde(default)]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub verb: String,
    #[serde(default)]
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

// --- Public Audit Logging Function ---

/// Creates a PhgitAudit resource in the cluster to record an event.
///
/// # Arguments
///
/// * `client` - A `kube::Client` to interact with the Kubernetes API.
/// * `verb` - The action performed, e.g., "grant", "revoke".
/// * `component` - The name of the phgit component generating the event, e.g., "rbac_manager".
/// * `actor` - An optional `Actor` struct identifying who performed the action.
/// * `target` - An optional `Target` struct identifying the resource the action was performed on.
/// * `details` - A map of additional, context-specific key-value pairs.
///
/// # Returns
///
/// A `Result` indicating success or failure.
pub async fn log_audit_event(
    client: Client,
    verb: String,
    component: String,
    actor: Option<Actor>,
    target: Option<Target>,
    details: BTreeMap<String, String>,
) -> Result<()> {
    let audits: Api<PhgitAudit> = Api::all(client);

    // Generate a unique, queryable name for the audit event resource.
    // The name is sanitized to be a valid Kubernetes resource name.
    let name = format!(
        "{}.{}.{}",
        component,
        verb,
        Utc::now().format("%Y%m%d%H%M%S%f")
    )
    .replace('_', "-")
    .to_lowercase();

    let audit_event = PhgitAudit::new(&name, PhgitAuditSpec {
        timestamp: Utc::now(),
        verb,
        component,
        actor,
        target,
        details,
    });

    audits.create(&PostParams::default(), &audit_event).await?;

    println!("[audit_logger] Successfully created audit event '{}'.", audit_event.metadata.name.unwrap());

    Ok(())
}
