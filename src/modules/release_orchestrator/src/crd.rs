/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/crd.rs
*
* This file defines the Rust data structures for the PhgitRelease Custom Resource.
* This provides a typed interface for interacting with PhgitRelease objects in
* the Kubernetes API.
*
* SPDX-License-Identifier: Apache-2.0 */

use kube::CustomResource;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitRelease",
    namespaced,
    status = "PhgitReleaseStatus",
    shortname = "pgrls"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitReleaseSpec {
    pub app_name: String,
    pub version: String,
    pub strategy: Strategy,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Strategy {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canary: Option<CanaryStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blue_green: Option<BlueGreenStrategy>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanaryStrategy {
    pub steps: Vec<CanaryStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<Analysis>,
    #[serde(default)]
    pub auto_promote: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanaryStep {
    pub set_weight: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_window: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlueGreenStrategy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    #[serde(default)]
    pub auto_promote: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    #[serde(default = "default_interval")]
    pub interval: String,
    #[serde(default = "default_threshold")]
    pub threshold: i32,
    #[serde(default = "default_max_failures")]
    pub max_failures: i32,
    pub metrics: Vec<Metric>,
}

fn default_interval() -> String { "1m".to_string() }
fn default_threshold() -> i32 { 5 }
fn default_max_failures() -> i32 { 2 }

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub name: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_success: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhgitReleaseStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_to: Option<i32>,
}
