/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: k8s/operators/ph_operator/src/crds/autoheal_rule.rs
 *
 * This file defines the Rust data structures for the phAutoHealRule
 * Custom Resource Definition.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use kube::CustomResource;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[kube(
    group = "ph.kaiser.io",
    version = "v1alpha1",
    kind = "phAutoHealRule",
    namespaced,
    status = "PhAutoHealRuleStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct PhAutoHealRuleSpec {
    pub trigger_name: String,
    pub cooldown: String,
    pub actions: Vec<HealAction>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct HealAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redeploy: Option<RedeployAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_up: Option<ScaleUpAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runbook: Option<RunbookAction>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct RedeployAction {
    pub target: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScaleUpAction {
    pub target: String,
    pub replicas: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunbookAction {
    pub script_name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct PhAutoHealRuleStatus {
    // Status fields can be added here in the future if needed.
    // For example, last execution time, status, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_execution_time: Option<String>,
}
