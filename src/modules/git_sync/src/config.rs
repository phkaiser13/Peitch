/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/git_sync/src/config.rs
*
* This file has been refactored to support distinct actions (`sync`, `drift`)
* with their own specific configurations, matching the payloads from the C CLI
* handlers. It uses a top-level enum `GitOpsPayload` to deserialize the
* incoming JSON and dispatch to the correct typed configuration.
*
* SPDX-License-Identifier: Apache-2.0 */

use serde::Deserialize;

/// This enum represents the top-level JSON payload sent from the C handler.
/// It allows `serde` to figure out which action is being requested and deserialize
/// the rest of the payload into the appropriate struct.
#[derive(Deserialize, Debug)]
#[serde(tag = "action")]
pub enum GitOpsPayload {
    #[serde(rename = "sync")]
    Sync(SyncConfig),
    #[serde(rename = "drift")]
    Drift(DriftConfig),
}

/// Configuration for the 'sync' action.
/// Fields correspond to the JSON created by `handle_sync_command` in C.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    pub path: String,
    pub cluster: String,
    pub context: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub apply: bool,
    #[serde(default)]
    pub skip_signature_verification: bool,
}

/// Configuration for the 'drift' action.
/// Fields correspond to the JSON created by `handle_drift_command` in C.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriftConfig {
    pub path: String,
    pub cluster: String,
    pub since: Option<String>,
    pub label: Option<String>,
    pub open_pr: bool,
    pub auto_apply: bool,
}