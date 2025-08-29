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
    group = "phgit.dev",
    version = "v1",
    kind = "PhgitRelease",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitReleaseSpec {
    pub name: String,
    pub source: String, // e.g., "repo@rev"
    pub artifacts: Vec<Artifact>,
    pub strategy: String, // e.g., "canary"
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub image: String,
    #[serde(default)]
    pub sbom: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhgitReleaseStatus {
    #[serde(default)]
    pub state: String, // e.g., "InProgress", "Promoted", "Failed"
    #[serde(default)]
    pub promoted_at: String,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    #[serde(default)]
    pub signer: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub verification_timestamp: String,
}
