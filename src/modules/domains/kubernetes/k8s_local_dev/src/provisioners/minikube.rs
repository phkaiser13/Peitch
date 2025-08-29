/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_local_dev/src/provisioners/minikube.rs
 * Implements the `Provisioner` trait for minikube using the `minikube` CLI.
 * SPDX-License-Identifier: Apache-2.0
 */

use super::{common::execute_command, Provisioner};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

/// Represents the 'minikube' provisioner.
pub struct MinikubeProvisioner;

#[async_trait]
impl Provisioner for MinikubeProvisioner {
    /// Creates a minikube cluster via:
    /// `minikube start -p <name> [--kubernetes-version=<version>]`
    async fn create(&self, name: &str, k8s_version: &str) -> Result<()> {
        let mut command = Command::new("minikube");
        command.arg("start").arg("-p").arg(name);

        if !k8s_version.is_empty() {
            let arg = format!("--kubernetes-version={}", k8s_version);
            command.arg(arg);
        }

        execute_command(&mut command)
            .await
            .context("Failed to execute 'minikube start'")?;

        Ok(())
    }

    /// Deletes a minikube cluster via:
    /// `minikube delete -p <name>`
    async fn delete(&self, name: &str) -> Result<()> {
        let mut command = Command::new("minikube");
        command.arg("delete").arg("-p").arg(name);

        execute_command(&mut command)
            .await
            .context("Failed to execute 'minikube delete'")?;

        Ok(())
    }

    /// Lists minikube profiles via:
    /// `minikube profile list`
    async fn list(&self) -> Result<()> {
        let mut command = Command::new("minikube");
        command.arg("profile").arg("list");

        execute_command(&mut command)
            .await
            .context("Failed to execute 'minikube profile list'")?;

        Ok(())
    }
}
