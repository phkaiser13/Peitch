/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/git_sync/src/git_ops.rs
 *
 * This file is responsible for all Git and GitHub operations required to reconcile
 * configuration drift. Its primary function, `create_reconciliation_pr`, takes the
 * detected drift details, programmatically creates a new Git branch, applies the
 * correct configurations to files in the local repository clone, commits, pushes to the
 * remote, and then opens a pull request on GitHub.
 *
 * Architecture:
 * The implementation is carefully designed to be robust, secure, and non-blocking.
 * 1. Git Operations (`git2`): For interacting with the local Git repository, this module
 * uses the `git2` crate, which provides safe Rust bindings to the battle-tested `libgit2`
 * library. This is a superior approach to shelling out to the `git` command-line tool, as
 * it avoids command injection vulnerabilities, provides better error handling, and offers
 * a structured, programmatic API.
 * 2. GitHub API (`octocrab`): For creating the pull request, the `octocrab` crate is
 * utilized. It is a modern, `async`-native GitHub API client that integrates seamlessly
 * with the Tokio runtime, ensuring that network operations do not block execution.
 * 3. Configuration and Security: All sensitive information, such as the GitHub Personal
 * Access Token (PAT), is passed as arguments and not hardcoded. The function requires
 * a PAT with `repo` scope to create branches and pull requests.
 * 4. Asynchronicity and I/O: File system operations (writing the corrected Kubernetes
 * manifests) are performed using `tokio::fs` to maintain the asynchronous nature of the
 * application.
 * 5. Error Handling: Comprehensive error handling is implemented using `anyhow::Result`.
 * Each step—opening the repo, creating a branch, writing files, committing, pushing, and
 * creating the PR—is equipped with context-rich error reporting to simplify debugging.
 *
 * Process Flow:
 * - A collection of `DriftDetail` structs is received as input.
 * - A unique, timestamped branch name is generated (e.g., "reconciliation/2025-08-25T142015Z").
 * - The local Git repository is opened.
 * - The new branch is created from the HEAD of the default branch (e.g., `main` or `master`).
 * - The working directory is updated to reflect the new branch.
 * - The corrected Kubernetes YAML configurations (`live_yaml` from `DriftDetail`) are
 * written to their corresponding file paths, overwriting the drifted files.
 * - All changes in the working directory are staged for commit.
 * - A commit is created with a standardized message detailing the automated reconciliation.
 * - The new branch with its commit is pushed to the remote repository (`origin`).
 * - Using the GitHub API, a pull request is created from the new branch to the default branch.
 * - The URL of the newly created pull request is returned upon success.
 *
 * This modular separation ensures that `git_ops.rs` focuses solely on the "Act" phase of
 * the GitOps loop, leaving detection to `kube_diff.rs`.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::kube_diff::DriftDetail;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use git2::{Commit, Cred, PushOptions, RemoteCallbacks, Repository, Signature};
use octocrab::Octocrab;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Encapsulates the context required for GitHub operations.
pub struct GitHubContext<'a> {
    pub token: &'a str,
    pub repo_owner: &'a str,
    pub repo_name: &'a str,
}

/// Creates a new branch, applies drift corrections, commits, pushes, and creates a pull request.
///
/// # Arguments
/// * `drifts` - A vector of `DriftDetail` structs containing the detected drifts.
/// * `git_repo_path` - The file system path to the local clone of the Git repository.
/// * `github_context` - A struct containing the GitHub repository owner, name, and PAT.
///
/// # Returns
/// A `Result` containing the URL of the created pull request as a `String`.
pub async fn create_reconciliation_pr(
    drifts: &[DriftDetail],
    git_repo_path: &Path,
    github_context: &GitHubContext<'_>,
) -> Result<String> {
    if drifts.is_empty() {
        println!("No drift detected. No pull request to create.");
        return Ok("No drift detected.".to_string());
    }

    // 1. Generate a unique, timestamped branch name.
    let now: DateTime<Utc> = Utc::now();
    let branch_name = format!("reconciliation/{}", now.format("%Y-%m-%dT%H%M%SZ"));

    // 2. Perform Git operations: create branch, write files, commit, and push.
    // This is a blocking operation, so we wrap it in `tokio::task::spawn_blocking`
    // to avoid blocking the async runtime. This is crucial because `git2` is a sync library.
    let repo_path_clone = git_repo_path.to_path_buf();
    let branch_name_clone = branch_name.clone();
    let drifts_owned: Vec<DriftDetail> = drifts.iter().cloned().collect(); // Clone drifts to move into the closure
    let github_token_clone = github_context.token.to_string(); // Clone token to move into the closure

    tokio::task::spawn_blocking(move || {
        perform_git_operations(
            &drifts_owned,
            &repo_path_clone,
            &branch_name_clone,
            &github_token_clone,
        )
    }).await.context("Git operation task failed to complete.")??;

    // 3. Create the pull request using the GitHub API.
    let pr_title = format!("Automated Reconciliation: Fix {} Drifted Resources", drifts.len());
    let pr_body = generate_pr_body(drifts);

    let pr_url = create_github_pull_request(
        github_context,
        &pr_title,
        &pr_body,
        &branch_name,
        "main", // Assuming 'main' is the default branch. This could be made configurable.
    ).await?;

    Ok(pr_url)
}

/// Handles all local Git operations using the `git2` crate.
fn perform_git_operations(
    drifts: &[DriftDetail],
    repo_path: &Path,
    branch_name: &str,
    token: &str,
) -> Result<()> {
    // Open the repository
    let repo = Repository::open(repo_path)
        .with_context(|| format!("Failed to open Git repository at '{}'", repo_path.display()))?;

    // Get the HEAD commit to branch from
    let head = repo.head()?.peel_to_commit()
        .context("Failed to find HEAD commit. The repository might be empty.")?;

    // Create the new branch
    repo.branch(branch_name, &head, false)
        .with_context(|| format!("Failed to create branch '{}'", branch_name))?;

    // Checkout the new branch
    let obj = repo.revparse_single(&format!("refs/heads/{}", branch_name))?;
    repo.checkout_tree(&obj, None)
        .context("Failed to checkout tree for new branch.")?;
    repo.set_head(&format!("refs/heads/{}", branch_name))
        .context("Failed to set HEAD to new branch.")?;

    // Apply file changes (by overwriting)
    for drift in drifts {
        let file_path = repo_path.join(&drift.git_path);
        std::fs::write(&file_path, &drift.live_yaml)
            .with_context(|| format!("Failed to write updated manifest to '{}'", file_path.display()))?;
    }

    // Stage all changes
    let mut index = repo.index().context("Failed to get repository index.")?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .context("Failed to stage changes.")?;
    index.write().context("Failed to write index.")?;
    let tree_id = index.write_tree().context("Failed to write tree.")?;
    let tree = repo.find_tree(tree_id)?;

    // Create the commit
    let signature = Signature::now("peitch Automation", "bot@peitch.dev")
        .context("Failed to create Git signature.")?;
    let parent_commit = repo.head()?.peel_to_commit()?;

    let commit_message = format!(
        "Automated Reconciliation: Fix {} drifted resources\n\nThis commit was generated automatically to bring the cluster state back in sync with the Git source of truth.",
        drifts.len()
    );

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &commit_message,
        &tree,
        &[&parent_commit],
    ).context("Failed to create commit.")?;

    println!("Successfully created commit for branch '{}'", branch_name);

    // Push the new branch to the remote
    let mut remote = repo.find_remote("origin")
        .context("Failed to find remote 'origin'.")?;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, _username_from_url, _allowed_types| {
        // We use the GitHub token for authentication.
        Cred::userpass_plaintext(token, "")
    });

    let mut push_options = PushOptions::new();
    push_options.remote_callbacks(callbacks);

    remote.push(
        &[format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name)],
        Some(&mut push_options),
    ).context("Failed to push branch to remote.")?;

    println!("Successfully pushed branch '{}' to origin.", branch_name);

    Ok(())
}


/// Creates a pull request on GitHub using the `octocrab` client.
async fn create_github_pull_request(
    context: &GitHubContext<'_>,
    title: &str,
    body: &str,
    head_branch: &str,
    base_branch: &str,
) -> Result<String> {
    let octocrab = Octocrab::builder()
        .personal_token(context.token.to_string())
        .build()?;

    let pr = octocrab
        .pulls(context.repo_owner, context.repo_name)
        .create(title, head_branch, base_branch)
        .body(body)
        .send()
        .await
        .context("Failed to create pull request on GitHub.")?;

    println!("Successfully created pull request: {}", pr.html_url);
    Ok(pr.html_url.to_string())
}

/// Generates a markdown-formatted body for the pull request.
fn generate_pr_body(drifts: &[DriftDetail]) -> String {
    let mut body = "## 🤖 Automated Configuration Drift Reconciliation\n\n".to_string();
    body.push_str("This pull request was automatically generated by peitch's sync workflow to correct configuration drift detected between the live Kubernetes cluster and this repository.\n\n");
    body.push_str("The following resources were found to be out of sync and have been updated to match the live cluster state:\n\n");

    for drift in drifts {
        body.push_str(&format!(
            "### `{}` `{}/{}`\n",
            drift.kind, drift.namespace, drift.name
        ));
        body.push_str("```diff\n");
        body.push_str(&drift.patch);
        body.push_str("\n```\n\n");
    }

    body
}