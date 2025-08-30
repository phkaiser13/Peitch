/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: k8s/operators/ph_operator/src/controllers/utils.rs
*
* This file contains common utility functions shared across multiple controllers.
* Consolidating reusable logic here helps to keep the individual controller
* files focused on their specific reconciliation logic and reduces code duplication.
*
* Functions:
* - `replicate_secrets`: Replicates Secrets from a source to a destination cluster.
* - `replicate_configmaps`: Replicates ConfigMaps from a source to a destination cluster.
*
* SPDX-License-Identifier: Apache-2.0
*/

use anyhow::{Context, Result};
use futures::future::join_all;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::{
    api::{Api, ListParams, ObjectMeta, Patch, PatchParams},
    Client,
};
use tracing::{error, info, warn};

/// Replicates secrets from a source cluster to a destination cluster.
///
/// This function lists secrets in a given namespace of the source cluster that match
/// a label selector, and then creates equivalent secrets in the destination cluster.
pub async fn replicate_secrets(
    source_client: Client,
    dest_client: Client,
    namespace: &str,
    selector: &str,
) -> Result<()> {
    info!(
        "Starting secret replication from namespace '{}' with selector '{}'",
        namespace, selector
    );

    let source_api: Api<Secret> = Api::namespaced(source_client, namespace);
    let dest_api: Api<Secret> = Api::namespaced(dest_client, namespace);

    let lp = ListParams::default().labels(selector);
    let source_items = source_api.list(&lp).await.with_context(|| {
        format!(
            "Failed to list secrets in namespace '{}' with selector '{}'",
            namespace, selector
        )
    })?;

    if source_items.items.is_empty() {
        warn!(
            "No secrets found to replicate in namespace '{}' with selector '{}'.",
            namespace, selector
        );
        return Ok(());
    }

    info!("Found {} secret(s) to replicate.", source_items.items.len());

    let mut replication_futures = Vec::new();
    for secret in source_items.items {
        let dest_api_clone = dest_api.clone();
        replication_futures.push(async move {
            let item_name = secret.metadata.name.as_deref().unwrap_or("unknown");
            info!("Replicating secret '{}'...", item_name);

            let new_item = Secret {
                metadata: ObjectMeta {
                    name: secret.metadata.name,
                    namespace: secret.metadata.namespace,
                    labels: secret.metadata.labels,
                    annotations: secret.metadata.annotations,
                    ..Default::default()
                },
                data: secret.data,
                type_: secret.type_,
                ..Default::default()
            };

            let ssapply = PatchParams::apply("ph.resource-replicator");
            dest_api_clone
                .patch(item_name, &ssapply, &Patch::Apply(&new_item))
                .await
                .with_context(|| format!("Failed to apply secret '{}' to destination cluster", item_name))
        });
    }

    let results = join_all(replication_futures).await;
    let mut errors = Vec::new();
    for result in results {
        if let Err(e) = result {
            error!("Secret replication failed: {:?}", e);
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(anyhow::anyhow!("{} secret(s) failed to replicate.", errors.len()));
    }

    info!("Successfully replicated all targeted secrets.");
    Ok(())
}

/// Replicates ConfigMaps from a source cluster to a destination cluster.
///
/// This function lists ConfigMaps in a given namespace of the source cluster that match
/// a label selector, and then creates equivalent ConfigMaps in the destination cluster.
pub async fn replicate_configmaps(
    source_client: Client,
    dest_client: Client,
    namespace: &str,
    selector: &str,
) -> Result<()> {
    info!(
        "Starting ConfigMap replication from namespace '{}' with selector '{}'",
        namespace, selector
    );

    let source_api: Api<ConfigMap> = Api::namespaced(source_client, namespace);
    let dest_api: Api<ConfigMap> = Api::namespaced(dest_client, namespace);

    let lp = ListParams::default().labels(selector);
    let source_items = source_api.list(&lp).await.with_context(|| {
        format!(
            "Failed to list ConfigMaps in namespace '{}' with selector '{}'",
            namespace, selector
        )
    })?;

    if source_items.items.is_empty() {
        warn!(
            "No ConfigMaps found to replicate in namespace '{}' with selector '{}'.",
            namespace, selector
        );
        return Ok(());
    }

    info!("Found {} ConfigMap(s) to replicate.", source_items.items.len());

    let mut replication_futures = Vec::new();
    for cm in source_items.items {
        let dest_api_clone = dest_api.clone();
        replication_futures.push(async move {
            let item_name = cm.metadata.name.as_deref().unwrap_or("unknown");
            info!("Replicating ConfigMap '{}'...", item_name);

            let new_item = ConfigMap {
                metadata: ObjectMeta {
                    name: cm.metadata.name,
                    namespace: cm.metadata.namespace,
                    labels: cm.metadata.labels,
                    annotations: cm.metadata.annotations,
                    ..Default::default()
                },
                data: cm.data,
                binary_data: cm.binary_data,
                ..Default::default()
            };

            let ssapply = PatchParams::apply("ph.resource-replicator");
            dest_api_clone
                .patch(item_name, &ssapply, &Patch::Apply(&new_item))
                .await
                .with_context(|| format!("Failed to apply ConfigMap '{}' to destination cluster", item_name))
        });
    }

    let results = join_all(replication_futures).await;
    let mut errors = Vec::new();
    for result in results {
        if let Err(e) = result {
            error!("ConfigMap replication failed: {:?}", e);
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(anyhow::anyhow!("{} ConfigMap(s) failed to replicate.", errors.len()));
    }

    info!("Successfully replicated all targeted ConfigMaps.");
    Ok(())
}
