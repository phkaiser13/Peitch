/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: k8s/operators/ph_operator/src/controllers/rbac_policy_controller.rs
*
* This file implements the controller for the `PhgitRbacPolicy` Custom Resource.
* Its purpose is to provide a declarative, GitOps-friendly way to manage RBAC.
*
* Architecture:
* - The controller watches for `PhgitRbacPolicy` resources.
* - For each policy, it reconciles the state by creating or updating a
*   corresponding `RoleBinding` in the same namespace.
* - The `RoleBinding` links the policy's subject (User or Group) to a
*   pre-defined `ClusterRole`.
* - A finalizer ensures that when a `PhgitRbacPolicy` is deleted, the associated
*   `RoleBinding` is also garbage collected, preventing orphaned permissions.
*
* SPDX-License-Identifier: Apache-2.0
*/

use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams, PostParams, Resource, finalizer},
    client::Client,
    runtime::controller::{Action, Controller},
    CustomResource,
};
use k8s_openapi::api::rbac::v1 as rbac;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use thiserror::Error;
use tokio::time::Duration;
use tracing::{info, warn};

// --- Custom Resource Definition ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitRbacPolicy",
    namespaced
)]
#[kube(status = "PhgitRbacPolicyStatus")]
pub struct PhgitRbacPolicySpec {
    pub role: String,
    pub subject: Subject,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct Subject {
    pub kind: String,
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct PhgitRbacPolicyStatus {
    #[serde(rename = "bindingName")]
    pub binding_name: Option<String>,
    pub conditions: Vec<StatusCondition>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct StatusCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    #[serde(rename = "lastTransitionTime")]
    pub last_transition_time: String,
    pub message: String,
}

// --- Error Handling ---

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Finalizer error: {0}")]
    FinalizerError(#[source] Box<dyn std::error::Error + Send + Sync>),
    
    #[error("Invalid role '{0}'. It is not defined in the static role map.")]
    InvalidRole(String),
}

// --- Controller Context and Static Data ---

struct Context {
    client: Client,
}

static ROLE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("promoter", "ph-cluster-promoter");
    m.insert("preview-admin", "ph-preview-administrator");
    m.insert("secrets-rotator", "ph-secrets-rotator");
    m
});

// --- Controller Entrypoint ---

pub async fn run(client: Client) {
    let api = Api::<PhgitRbacPolicy>::all(client.clone());
    Controller::new(api, Default::default())
        .run(reconcile, error_policy, Arc::new(Context { client }))
        .await;
}

// --- Reconciliation Logic ---

async fn reconcile(policy: Arc<PhgitRbacPolicy>, ctx: Arc<Context>) -> Result<Action, Error> {
    let ns = policy.namespace().unwrap(); // is namespaced
    let api = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&api, "phgitrbacpolicies.ph.io/binding-cleanup", policy, |event| async {
        match event {
            finalizer::Event::Apply(policy) => reconcile_policy(&policy, ctx.clone()).await,
            finalizer::Event::Cleanup(policy) => cleanup_policy(&policy, ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(e.into()))
}

async fn reconcile_policy(policy: &PhgitRbacPolicy, ctx: Arc<Context>) -> Result<Action> {
    let ns = policy.namespace().unwrap();
    let client = ctx.client.clone();
    let role_bindings: Api<rbac::RoleBinding> = Api::namespaced(client, &ns);

    let k8s_role_name = ROLE_MAP.get(policy.spec.role.as_str())
        .ok_or_else(|| Error::InvalidRole(policy.spec.role.clone()))?;

    let desired_binding = rbac::RoleBinding {
        metadata: ObjectMeta {
            name: Some(format!("ph-policy-{}", policy.name_any())),
            namespace: Some(ns.clone()),
            owner_references: Some(vec![policy.controller_owner_ref(&()).unwrap()]),
            ..Default::default()
        },
        role_ref: rbac::RoleRef {
            api_group: "rbac.authorization.k8s.io".to_string(),
            kind: "ClusterRole",
            name: k8s_role_name.to_string(),
        },
        subjects: Some(vec![rbac::Subject {
            kind: policy.spec.subject.kind.clone(),
            name: policy.spec.subject.name.clone(),
            api_group: Some("rbac.authorization.k8s.io".to_string()),
            ..Default::default()
        }]),
    };

    info!("Reconciling PhgitRbacPolicy '{}' in namespace '{}'", policy.name_any(), ns);
    role_bindings.patch(
        &desired_binding.name_any(),
        &PatchParams::apply("rbac-policy-controller.ph.io"),
        &Patch::Apply(&desired_binding),
    ).await?;

    Ok(Action::requeue(Duration::from_secs(3600)))
}

async fn cleanup_policy(policy: &PhgitRbacPolicy, ctx: Arc<Context>) -> Result<Action> {
    let ns = policy.namespace().unwrap();
    let client = ctx.client.clone();
    let role_bindings: Api<rbac::RoleBinding> = Api::namespaced(client, &ns);
    let binding_name = format!("ph-policy-{}", policy.name_any());

    info!("Deleting RoleBinding '{}' for PhgitRbacPolicy '{}'", binding_name, policy.name_any());
    match role_bindings.delete(&binding_name, &Default::default()).await {
        Ok(_) => Ok(Action::await_change()),
        Err(kube::Error::Api(e)) if e.code == 404 => {
            // Already gone, we're done.
            Ok(Action::await_change())
        }
        Err(e) => Err(Error::KubeError(e)),
    }
}

fn error_policy(_policy: Arc<PhgitRbacPolicy>, error: &Error, _ctx: Arc<Context>) -> Action {
    warn!("Reconciliation failed: {}", error);
    Action::requeue(Duration::from_secs(60))
}
