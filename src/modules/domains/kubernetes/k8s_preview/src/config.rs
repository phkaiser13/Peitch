/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/k8s_preview/src/config.rs
*
* This file defines the data structures used for configuration within the
* k8s_preview module. It uses the `serde` crate to deserialize a JSON string,
* received from the C core via FFI, into strongly-typed Rust structs. This
* ensures that the input is validated at the boundary, preventing invalid
* data from propagating into the business logic.
*
* The configuration is designed to be flexible, with most fields being optional.
* This allows the C caller to provide only the necessary data for a specific
* action, reducing payload size and simplifying the FFI interface.
*
* SPDX-License-Identifier: Apache-2.0 */

use serde::Deserialize;

/// Represents the possible actions that can be performed by this module.
/// Using an enum provides compile-time safety, ensuring that only valid
/// actions are processed. This has been expanded to include the full suite
/// of CLI commands.
///
/// The `#[serde(rename_all = "snake_case")]` attribute instructs serde to
/// match JSON string values (e.g., "create", "status") to the enum variants.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Destroy,
    Status,
    Logs,
    Exec,
    Extend,
    Gc, // Garbage Collect
}

/// Represents the main configuration structure for the preview environment task.
/// An instance of this struct is created by deserializing the JSON configuration
/// string passed from the C core. All fields except `action` are optional to

/// accommodate the different requirements of each action.
#[derive(Deserialize, Debug)]
pub struct PreviewConfig {
    /// The action to be performed. This is the only mandatory field.
    pub action: Action,

    /// The unique identifier for the pull request. Required for most actions
    /// that target a specific preview environment (create, destroy, status, etc.).
    /// Not used by 'gc'.
    pub pr_number: Option<u32>,

    /// The URL of the git repository to be deployed.
    /// Required only for the 'create' action.
    pub git_repo_url: Option<String>,

    /// The specific commit SHA that triggered the action, ensuring reproducibility.
    /// Required only for the 'create' action.
    pub commit_sha: Option<String>,

    /// An optional path to a kubeconfig file. If `None`, the client will attempt
    /// to use in-cluster configuration or the default user configuration.
    pub kubeconfig_path: Option<String>,

    // --- Action-specific fields ---
    /// The name of the component (e.g., a specific pod/deployment) to target.
    /// Used by 'logs' and 'exec' actions.
    pub component_name: Option<String>,

    /// The command and its arguments to execute inside a component's container.
    /// Represented as a list of strings (e.g., `["/bin/sh", "-c", "ls -la"]`).
    /// Used by the 'exec' action.
    pub command_to_exec: Option<Vec<String>>,

    /// The new Time-To-Live (in hours) to set for a preview environment.
    /// Used by the 'extend' action.
    pub new_ttl: Option<u32>,

    /// The maximum age (in hours) for preview environments before they are
    /// considered for garbage collection. Used by the 'gc' action.
    pub max_age_hours: Option<u32>,
}