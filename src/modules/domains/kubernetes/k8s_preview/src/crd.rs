/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/k8s_preview/src/crd.rs
*
* This file defines the Rust data structures that correspond to the phPreview
* Custom Resource Definition (CRD). By using the `kube::CustomResource` derive
* macro, we create a typed API for our custom resource, which allows the Rust
* compiler to enforce correctness and provides strong type safety when
* interacting with the Kubernetes API.
*
* The structs defined here (`phPreview`, `phPreviewSpec`, `phPreviewStatus`)
* directly map to the OpenAPI v3 schema in the `ph.io_phpreviews.yaml` CRD file.
* This mapping is what enables `kube-rs` to serialize and deserialize phPreview
* objects into the correct format for Kubernetes.
*
* SPDX-License-Identifier: Apache-2.0 */

use kube::CustomResource;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Defines the Custom Resource structure for a phPreview.
///
/// The `#[kube(..)]` attributes are used by the `CustomResource` derive macro
/// to configure the resource's API group, version, kind, and other metadata.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "phPreview",
    namespaced
)]
#[kube(
    plural = "phpreviews",
    singular = "phpreview",
    shortname = "pgprv"
)]
#[serde(rename_all = "camelCase")]
pub struct phPreviewSpec {
    /// The number of the pull request this preview is for.
    pub pr_number: i32,
    /// The URL of the Git repository.
    pub repo_url: String,
    /// The specific Git commit SHA to be deployed.
    pub commit_sha: String,
    /// Time-to-live in hours for the preview environment.
    #[serde(default = "default_ttl")]
    pub ttl_hours: i32,
    /// Path within the repository to the Kubernetes manifests to apply.
    #[serde(default = "default_manifest_path")]
    pub manifest_path: String,
}

/// A default value for the `ttl_hours` field.
fn default_ttl() -> i32 { 24 }

/// A default value for the `manifest_path` field.
fn default_manifest_path() -> String { "./k8s".to_string() }

/// Defines the observed status of a phPreview resource.
/// This part of the resource is updated by the controller, not the user.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct phPreviewStatus {
    /// The current phase of the preview environment (e.g., Creating, Ready, Deleting, Error).
    #[serde(default)]
    pub phase: String,
    /// The accessible URL for the preview environment once it is ready.
    #[serde(default)]
    pub url: String,
    /// Timestamp indicating when the environment is scheduled for deletion.
    #[serde(default)]
    pub expires_at: String,
    /// A human-readable message describing the current status or any errors.
    #[serde(default)]
    pub message: String,
}
