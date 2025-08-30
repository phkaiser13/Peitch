/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_local_dev/src/provisioners/kind.rs
 * Implements the `Provisioner` trait for kind (Kubernetes in Docker).
 * SPDX-License-Identifier: Apache-2.0
 */

use super::{common::execute_command, Provisioner};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

/// Represents the 'kind' provisioner backed by the kind CLI.
pub struct KindProvisioner;

#[async_trait]
impl Provisioner for KindProvisioner {
    /// Creates a kind cluster via `kind create cluster --name <name> [--image kindest/node:v<ver>]`.
    /// If `k8s_version` is empty, the `--image` flag is omitted, using the kind default.
    async fn create(&self, name: &str, k8s_version: &str) -> Result<()> {
        let mut command = Command::new("kind");
        command.arg("create").arg("cluster").arg("--name").arg(name);

        if !k8s_version.is_empty() {
            let image = format!("kindest/node:v{}", k8s_version);
            command.arg("--image").arg(image);
        }

        execute_command(&mut command)
            .await
            .context("Failed to execute 'kind create cluster'")?;

        Ok(())
    }

    /// Deletes a kind cluster via `kind delete cluster --name <name>`.
    /// This function was missing its implementation.
    async fn delete(&self, name: &str) -> Result<()> {
        let mut command = Command::new("kind");
        command.arg("delete").arg("cluster").arg("--name").arg(name);

        execute_command(&mut command)
            .await
            .context("Failed to execute 'kind delete cluster'")?;

        Ok(())
    }

    /// Lists kind clusters via `kind get clusters`.
    async fn list(&self) -> Result<()> {
        let mut command = Command::new("kind");
        command.arg("get").arg("clusters");

        execute_command(&mut command)
            .await
            .context("Failed to execute 'kind get clusters'")?;

        Ok(())
    }
}
