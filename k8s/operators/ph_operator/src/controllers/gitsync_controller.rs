// Copyright (C) 2025 Pedro Henrique / phkaiser13
//
// File: k8s/operators/ph_operator/src/controllers/gitsync_controller.rs
//
// This file implements the reconciliation logic for the PhgitSyncJob custom resource.
// It is responsible for orchestrating the synchronization of manifests from a
// Git repository to a target cluster.
//
// SPDX-License-Identifier: Apache-2.0

use crate::crds::{PhgitSyncJob, PhgitSyncJobStatus, SyncJobPhase, StatusCondition};
use kube::{
    api::{Api, Patch, PatchParams},
    client::Client,
    runtime::controller::Action,
    Resource, ResourceExt,
};
use std::sync::Arc;
use tokio::time::Duration;
use thiserror::Error;
use serde_json::json;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),
}

pub struct Context {
    pub client: Client,
}

use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{DynamicObject, GroupVersionKind};
use kube::{discovery, Api, Client};
use std::collections::BTreeMap;
use anyhow::{anyhow, Context as AnyhowContext};

pub async fn reconcile(job: Arc<PhgitSyncJob>, ctx: Arc<Context>) -> Result<Action, Error> {
    let ns = job.namespace().unwrap();
    let api: Api<PhgitSyncJob> = Api::namespaced(ctx.client.clone(), &ns);
    let job_name = job.name_any();

    // Set status to Syncing
    let new_status = PhgitSyncJobStatus {
        phase: Some(SyncJobPhase::Syncing),
        start_time: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    };
    let patch = Patch::Apply(json!({ "status": new_status }));
    api.patch_status(&job_name, &PatchParams::apply("ph-gitsync-controller"), &patch).await?;

    tracing::info!(job = %job_name, "Syncing manifests from path: {}", job.spec.path);

    // Read manifests from the path specified in the spec
    let manifests = match tokio::fs::read_to_string(&job.spec.path).await {
        Ok(m) => m,
        Err(e) => {
            let error_message = format!("Failed to read manifests from path {}: {}", job.spec.path, e);
            update_status_with_error(&api, &job_name, &error_message).await?;
            return Ok(Action::await_change());
        }
    };

    // The actual apply logic, moved from the old cluster_manager
    match execute_apply(ctx.client.clone(), &manifests, &BTreeMap::new()).await {
        Ok(applied_resources) => {
            let success_message = format!("Successfully applied {} resources.", applied_resources.len());
            update_status_with_success(&api, &job_name, &success_message).await?;
        }
        Err(e) => {
            let error_message = format!("Failed to apply manifests: {}", e);
            update_status_with_error(&api, &job_name, &error_message).await?;
        }
    }

    Ok(Action::await_change())
}

/// Private helper to apply manifests to a single cluster.
async fn execute_apply(
    client: Client,
    manifests: &str,
    variables: &BTreeMap<String, String>,
) -> AnyhowResult<Vec<String>> {
    let ssapply = PatchParams::apply("ph.gitsync-controller");
    let mut applied_resources = Vec::new();

    let mut templated_manifests = manifests.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{ .{} }}}}", key);
        templated_manifests = templated_manifests.replace(&placeholder, value);
    }

    for doc in serde_yaml::Deserializer::from_str(&templated_manifests) {
        let obj: DynamicObject = serde::Deserialize::deserialize(doc)
            .context("Failed to deserialize YAML manifest into a Kubernetes object")?;

        let gvk = obj.gvk().context("Resource is missing GroupVersionKind")?;
        let name = obj.name_any();
        let namespace = obj.namespace();

        let (ar, _caps) = discovery::pinned_kind(&client, &gvk).await
            .with_context(|| format!("Failed to discover API resource for GVK: {}", gvk))?;

        let api: Api<DynamicObject> = if let Some(ns) = &namespace {
            Api::namespaced_with(client.clone(), ns, &ar)
        } else {
            Api::all_with(client.clone(), &ar)
        };

        api.patch(&name, &ssapply, &Patch::Apply(&obj)).await
            .with_context(|| format!("Failed to apply resource '{}/{}'", gvk, name))?;

        applied_resources.push(format!("{}/{}", gvk, name));
    }

    if applied_resources.is_empty() {
        return Err(anyhow!("No valid Kubernetes resources found in manifests."));
    }

    Ok(applied_resources)
}

async fn update_status_with_error(api: &Api<PhgitSyncJob>, name: &str, error_message: &str) -> Result<(), Error> {
    let new_status = PhgitSyncJobStatus {
        phase: Some(SyncJobPhase::Failed),
        completion_time: Some(chrono::Utc::now().to_rfc3339()),
        conditions: vec![StatusCondition {
            type_: "Failed".to_string(),
            message: error_message.to_string(),
        }],
        ..Default::default()
    };
    let patch = Patch::Apply(json!({ "status": new_status }));
    api.patch_status(name, &PatchParams::apply("ph-gitsync-controller"), &patch).await?;
    Ok(())
}

async fn update_status_with_success(api: &Api<PhgitSyncJob>, name: &str, success_message: &str) -> Result<(), Error> {
    let new_status = PhgitSyncJobStatus {
        phase: Some(SyncJobPhase::Succeeded),
        completion_time: Some(chrono::Utc::now().to_rfc3339()),
        conditions: vec![StatusCondition {
            type_: "Succeeded".to_string(),
            message: success_message.to_string(),
        }],
        ..Default::default()
    };
    let patch = Patch::Apply(json!({ "status": new_status }));
    api.patch_status(name, &PatchParams::apply("ph-gitsync-controller"), &patch).await?;
    Ok(())
}

pub fn on_error(job: Arc<PhgitSyncJob>, error: &Error, _ctx: Arc<Context>) -> Action {
    tracing::error!(job = %job.name_any(), "Reconciliation failed: {}", error);
    // Requeue the job after a short delay on error.
    Action::requeue(Duration::from_secs(15))
}
