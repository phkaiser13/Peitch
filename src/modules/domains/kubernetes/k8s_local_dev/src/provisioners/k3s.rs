/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_local_dev/src/provisioners/k3s.rs
 * Implements the `Provisioner` trait for k3s using the `k3d` CLI.
 * SPDX-License-Identifier: Apache-2.0
 */

use super::{common::execute_command, Provisioner};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

/// Represents the 'k3s' provisioner backed by the k3d CLI.
pub struct K3sProvisioner;

#[async_trait]
impl Provisioner for K3sProvisioner {
    /// Creates a k3s cluster via `k3d cluster create <name> [--k3s-arg "..."]`.
    /// If `k8s_version` is empty, the `--k3s-arg` is omitted.
    async fn create(&self, name: &str, k8s_version: &str) -> Result<()> {
        let mut command = Command::new("k3d");
        command.arg("cluster").arg("create").arg(name);

        if !k8s_version.is_empty() {
            // k3d accepts k3s args via --k3s-arg; attach the k8s version to the server.
            let k8s_arg = format!("--k8s-version={}@server:0", k8s_version);
            command.arg("--k3s-arg").arg(k8s_arg);
        }

        execute_command(&mut command)
            .await
            .context("Failed to execute 'k3d cluster create'")?;

        Ok(())
    }

    /// Deletes a k3s cluster via `k3d cluster delete <name>`.
    async fn delete(&self, name: &str) -> Result<()> {
        let mut command = Command::new("k3d");
        command.arg("cluster").arg("delete").arg(name);

        execute_command(&mut command)
            .await
            .context("Failed to execute 'k3d cluster delete'")?;

        Ok(())
    }

    /// Lists k3d clusters via `k3d cluster list`.
    async fn list(&self) -> Result<()> {
        let mut command = Command::new("k3d");
        command.arg("cluster").arg("list");

        execute_command(&mut command)
            .await
            .context("Failed to execute 'k3d cluster list'")?;

        Ok(())
    }
}
