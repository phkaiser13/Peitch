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

use crate::crds::PhgitAudit;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams, Resource},
    client::Client,
    runtime::controller::{Action, Controller},
};
use serde_json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{info, warn};

const AUDIT_LOG_CONFIGMAP_NAME: &str = "ph-audit-log";
// This should ideally be the namespace the operator is running in.
const AUDIT_LOG_NAMESPACE: &str = "ph-system";

// --- Controller Context and Error Handling ---

struct Context {
    client: Client,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),
    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

// --- Controller Entrypoint ---

pub async fn run(client: Client) {
    let api = Api::<PhgitAudit>::all(client.clone());
    Controller::new(api, Default::default())
        .run(reconcile, error_policy, Arc::new(Context { client }))
        .await;
}

// --- Reconciliation Logic ---

async fn reconcile(audit: Arc<PhgitAudit>, ctx: Arc<Context>) -> Result<Action, Error> {
    info!(
        "Processing audit event '{}': [Component: {}, Verb: {}]",
        audit.name_any(),
        audit.spec.component,
        audit.spec.verb
    );

    let client = ctx.client.clone();
    let cm_api: Api<ConfigMap> = Api::namespaced(client, AUDIT_LOG_NAMESPACE);

    let event_name = audit.name_any();
    let event_content = serde_json::to_string(&audit.spec)?;

    // This implementation forwards audit events to a central ConfigMap.
    // NOTE: This has limitations. ConfigMaps have a size limit (~1MiB), so in a
    // high-traffic environment, this could fail. A production-grade solution would
    // involve log shipping to an external system (e.g., ELK, Splunk) or using
    // a more scalable storage solution. This implementation serves as a self-contained
    // demonstration of the forwarding principle.

    let patch = serde_json::json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": AUDIT_LOG_CONFIGMAP_NAME,
            "namespace": AUDIT_LOG_NAMESPACE,
        },
        "data": {
            event_name: event_content
        }
    });

    // Use Server-Side Apply to create the ConfigMap if it doesn't exist,
    // or to add/update a key if it does.
    cm_api
        .patch(
            AUDIT_LOG_CONFIGMAP_NAME,
            &PatchParams::apply("ph-audit-controller").force(),
            &Patch::Apply(&patch),
        )
        .await?;

    info!("Successfully forwarded audit event '{}' to ConfigMap '{}'", audit.name_any(), AUDIT_LOG_CONFIGMAP_NAME);

    // We don't requeue because audit events are immutable and processed once.
    Ok(Action::await_change())
}

fn error_policy(_audit: Arc<PhgitAudit>, error: &Error, _ctx: Arc<Context>) -> Action {
    warn!("Reconciliation failed: {}", error);
    // Don't requeue on error, as the event might be malformed or the log is full.
    Action::await_change()
}
