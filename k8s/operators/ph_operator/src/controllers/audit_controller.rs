/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: k8s/operators/ph_operator/src/controllers/audit_controller.rs
*
* This file implements the controller for the `PhgitAudit` Custom Resource.
* Its purpose is to watch for audit events created by other components and
* process them.
*
* Architecture:
* - The controller watches for `PhgitAudit` resources cluster-wide.
* - The current implementation simply logs the content of the audit event.
* - This provides a foundation that can be extended to forward audit events
*   to external logging systems (e.g., ELK, Splunk), enforce retention
*   policies by deleting old events, or trigger alerts on specific event types.
*
* NOTE: The struct definitions for PhgitAudit are duplicated from the
* `audit_logger` module. In a future refactoring, these should be moved to a
* shared `crds` crate to avoid this duplication.
*
* SPDX-License-Identifier: Apache-2.0
*/

use kube::{
    api::{Api, Resource},
    client::Client,
    runtime::controller::{Action, Controller},
    CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::info;

// --- Custom Resource Definition (Duplicated from audit_logger) ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitAudit",
    scope = "Cluster"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitAuditSpec {
    pub timestamp: String,
    pub verb: String,
    pub component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

// --- Controller Context and Error Handling ---

struct Context {
    client: Client,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),
}

// --- Controller Entrypoint ---

pub async fn run(client: Client) {
    let api = Api::<PhgitAudit>::all(client.clone());
    Controller::new(api, Default::default())
        .run(reconcile, error_policy, Arc::new(Context { client }))
        .await;
}

// --- Reconciliation Logic ---

async fn reconcile(audit: Arc<PhgitAudit>, _ctx: Arc<Context>) -> Result<Action, Error> {
    info!(
        "Processing audit event '{}': [Component: {}, Verb: {}]",
        audit.name_any(),
        audit.spec.component,
        audit.spec.verb
    );

    // This is where you would add logic to forward the audit event to an external system.
    // For now, we just log it and finish.

    // We don't requeue because audit events are immutable and processed once.
    Ok(Action::await_change())
}

fn error_policy(_audit: Arc<PhgitAudit>, error: &Error, _ctx: Arc<Context>) -> Action {
    tracing::warn!("Reconciliation failed: {}", error);
    // Don't requeue on error, as the event might be malformed.
    Action::await_change()
}
