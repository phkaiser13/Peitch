/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_local_dev/src/cli.rs
 * This file defines the entire command-line interface for the `k8s_local_dev`
 * tool using the `clap` crate. It uses a declarative, struct-based approach
 * to define commands, subcommands, and their arguments. This provides strong
 * typing, automatic validation, and generation of help messages from the
 * documentation comments.
 * SPDX-License-Identifier: Apache-2.0 */

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// A CLI tool to manage local Kubernetes development environments.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The command to execute.
    #[command(subcommand)]
    pub command: Commands,
}

/// The enumeration of available subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new local Kubernetes cluster.
    Create(CreateArgs),

    /// Delete an existing local Kubernetes cluster.
    Delete(DeleteArgs),

    /// List available local Kubernetes clusters.
    List(ListArgs),

    /// Aplica um perfil de aplicação (um conjunto de manifestos) ao cluster ativo.
    ApplyProfile(ApplyProfileArgs),

    /// Build, push, and deploy a local image and manifests for rapid development.
    Sync(SyncArgs),
}

/// Arguments for the `sync` command.
#[derive(Parser, Debug)]
pub struct SyncArgs {
    /// Path to the directory containing Kubernetes manifests to apply.
    #[arg(long)]
    pub path: PathBuf,

    /// The name of the container image to build and push to the local registry.
    #[arg(long)]
    pub image: String,
}

/// Arguments for the `create` command.
#[derive(Parser, Debug)]
pub struct CreateArgs {
    /// The name for the new cluster.
    #[arg(required = true)]
    pub cluster_name: String,

    /// The provider to use for creating the cluster.
    #[arg(long, value_enum, default_value_t = Provider::Kind)]
    pub provider: Provider,

    /// The version of Kubernetes to install.
    #[arg(long, default_value = "1.28.0")]
    pub k8s_version: String,

    /// Wait for the control plane to be ready before returning.
    #[arg(long, default_value_t = true)]
    pub wait: bool,
}

/// Arguments for the `delete` command.
#[derive(Parser, Debug)]
pub struct DeleteArgs {
    /// The name of the cluster to delete.
    #[arg(required = true)]
    pub cluster_name: String,

    /// The provider of the cluster to delete.
    #[arg(long, value_enum, default_value_t = Provider::Kind)]
    pub provider: Provider,
}

/// Arguments for the `list` command.
#[derive(Parser, Debug)]
pub struct ListArgs {
    /// The provider whose clusters should be listed.
    #[arg(long, value_enum, default_value_t = Provider::Kind)]
    pub provider: Provider,
}

/// Arguments for the `apply-profile` command.
#[derive(Parser, Debug)]
pub struct ApplyProfileArgs {
    /// The path to the directory containing the YAML manifests for the profile.
    /// Example: --profile-path ./profiles/my-app
    #[arg(long, short)]
    pub profile_path: PathBuf,

    /// Optionally allow specifying which provider the profile should be applied to.
    /// If omitted, the currently active/default kubeconfig context will be used.
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,
}

/// Defines the supported local Kubernetes providers.
/// Deriving `ValueEnum` allows `clap` to validate and suggest these values.
#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum Provider {
    /// Use 'kind' (Kubernetes IN Docker) as the provider.
    Kind,
    /// Use 'k3s' (Lightweight Kubernetes) as the provider.
    K3s,
    /// Use 'minikube' as the provider.
    Minikube,
}
