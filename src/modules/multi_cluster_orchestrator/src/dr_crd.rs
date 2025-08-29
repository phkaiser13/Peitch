/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/multi_cluster_orchestrator/src/dr_crd.rs
 *
 * This file defines the Rust data structures for the PhgitDisasterRecovery
 * Custom Resource Definition. These structs provide a type-safe way for the
 * multi_cluster_orchestrator and the dr_controller to interact with these
 * resources in the Kubernetes API.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use kube::CustomResource;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use chrono::{DateTime, Utc};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitDisasterRecovery",
    namespaced,
    status = "PhgitDisasterRecoveryStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitDisasterRecoverySpec {
    pub primary_cluster: ClusterConnection,
    pub dr_cluster: DrClusterConnection,
    pub target_application: TargetApplication,
    pub policy: DrPolicy,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClusterConnection {
    pub kubeconfig_secret_ref: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct DrClusterConnection {
    pub kubeconfig_secret_ref: String,
    #[serde(default = "default_replicas")]
    pub replicas: i32,
}

fn default_replicas() -> i32 { 3 }

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct TargetApplication {
    pub deployment_name: String,
    pub namespace: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct DrPolicy {
    pub health_check: HealthCheckPolicy,
    pub failover_trigger: FailoverTrigger,
    #[serde(default)]
    pub notification: NotificationPolicy,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckPolicy {
    pub prometheus_query: String,
    pub success_condition: String,
    pub interval: String,
    pub failure_threshold: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum FailoverTrigger {
    Automatic,
    Manual,
}

impl Default for FailoverTrigger {
    fn default() -> Self { FailoverTrigger::Automatic }
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhgitDisasterRecoveryStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_cluster: Option<ActiveCluster>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<DrState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_health_check_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub consecutive_failures: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ActiveCluster {
    Primary,
    Dr,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum DrState {
    Monitoring,
    Degraded,
    FailingOver,
    ActiveOnDR,
    Failed,
}
