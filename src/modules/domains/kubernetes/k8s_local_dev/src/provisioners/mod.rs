/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/k8s_local_dev/src/provisioners/mod.rs
* This file defines the abstraction layer for different local Kubernetes
* provisioners. It establishes a common `Provisioner` trait (interface) that
* decouples the main CLI logic from the specific implementation details of any
* given provider (e.g., kind, k3s, minikube). This allows for new provisioners
* to be added in a modular fashion.
* SPDX-License-Identifier: Apache-2.0 */

/// Declare the specific provisioner implementation modules.
pub mod kind;
pub mod k3s;
pub mod minikube;
pub mod common;

use crate::cli::Provider;
use anyhow::{anyhow, Result};
use async_trait::async_trait;

/// A trait defining the common interface for a local Kubernetes provisioner.
/// Each supported provider (kind, k3s, minikube, etc.) must implement this trait.
#[async_trait]
pub trait Provisioner {
    /// Creates a new cluster with the given name and Kubernetes version.
    async fn create(&self, name: &str, k8s_version: &str) -> Result<()>;

    /// Deletes a cluster with the given name.
    async fn delete(&self, name: &str) -> Result<()>;

    /// Lists all clusters managed by this provisioner.
    async fn list(&self) -> Result<()>;
}

/// Factory function to get a concrete implementation of a `Provisioner`.
///
/// Based on the provider selected via the CLI, this function returns a boxed
/// trait object representing the chosen provisioner.
///
/// # Arguments
/// * `provider` - The `Provider` enum variant from the CLI arguments.
pub fn get_provisioner(provider: Provider) -> Result<Box<dyn Provisioner + Send + Sync>> {
    match provider {
        Provider::Kind => Ok(Box::new(kind::KindProvisioner)),
        Provider::K3s => Ok(Box::new(k3s::K3sProvisioner)),
        Provider::Minikube => Ok(Box::new(minikube::MinikubeProvisioner)),
        _ => Err(anyhow!("Unsupported provider: {:?}", provider)),
    }
}
