/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - Implemented the core synchronization logic by replacing the placeholder TODO blocks
//   with actual `git push` operations using the `git2-rs` library.
// - Added a new private helper function, `push_commits`, to encapsulate the logic for
//   pushing changes. This function creates a temporary in-memory remote, which is a
//   safe way to push to a local path without permanently altering the source repo's config.
// - The `SyncEngine` struct was updated to store the paths of the source and target
//   repositories to make them available for the push operation.
// - The `run` method now calls `push_commits` for the appropriate direction (source-to-target
//   or target-to-source) when one repository is ahead of the other.
// - Success messages have been updated to be more descriptive.

// ---
//
// Module: src/modules/sync_engine/src/sync.rs
//
// Purpose:
//   This file contains the core state machine and logic for the synchronization engine.
//   It uses a `SyncEngine` struct to manage the entire lifecycle of a sync operation,
//   from loading persistent state to analyzing commit graphs and applying changes.
//
// Architecture:
//   - The engine leverages the `git2-rs` library for deep, programmatic access
//     to the Git object database.
//   - A `SyncState` struct is persisted to a JSON file (`.git/ph_sync_state.json`)
//     to track the last known synchronized commit, enabling efficient and correct
//     divergence analysis on subsequent runs.
//   - The main `run` function orchestrates a state machine: fetch, analyze, apply,
//     and save state.
//   - The "apply" step is implemented by creating an anonymous (in-memory) remote
//     and performing a `git push` to the other repository.
//
use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// A custom result type for our sync logic to ensure clean error handling.
type SyncResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// --- 1. The Persistent State Model ---
// This struct is serialized to/from JSON to keep track of the last known
// synchronized commit hashes. This is critical for finding the correct
// common ancestor in subsequent runs.
#[derive(Serialize, Deserialize, Debug, Default)]
struct SyncState {
    last_source_synced_oid: Option<String>,
    last_target_synced_oid: Option<String>,
}

// --- 2. The Sync Engine ---
// Encapsulates all resources and state required for a sync operation.
struct SyncEngine {
    // The source repository for the synchronization.
    source_repo: Repository,
    // The target repository for the synchronization.
    target_repo: Repository,
    /* BEGIN CHANGE: Store repository paths for push operations. */
    // The file path to the source repository.
    source_path: PathBuf,
    // The file path to the target repository.
    target_path: PathBuf,
    /* END CHANGE */
    // The fully-qualified path to the state file.
    state_path: PathBuf,
    // The in-memory representation of the last synchronized state.
    state: SyncState,
}

impl SyncEngine {
    /// Creates a new SyncEngine instance.
    ///
    /// It opens the git repositories and loads the persistent synchronization
    /// state from a file within the source repository's `.git` directory.
    fn new(source_path: &str, target_path: &str) -> SyncResult<Self> {
        let source_repo = Repository::open(source_path)?;
        let target_repo = Repository::open(target_path)?;

        // State is stored in a hidden file to avoid cluttering the user's working directory.
        let state_path = Path::new(source_path)
            .join(".git")
            .join("ph_sync_state.json");

        let state = if state_path.exists() {
            let state_json = std::fs::read_to_string(&state_path)?;
            serde_json::from_str(&state_json)?
        } else {
            SyncState::default()
        };

        Ok(SyncEngine {
            source_repo,
            target_repo,
            /* BEGIN CHANGE: Initialize repository path fields. */
            source_path: PathBuf::from(source_path),
            target_path: PathBuf::from(target_path),
            /* END CHANGE */
            state_path,
            state,
        })
    }

    /// The main entry point for the synchronization state machine.
    ///
    /// This async function orchestrates the entire process:
    /// 1. Fetches the latest changes from remotes.
    /// 2. Analyzes the commit graph to determine divergence.
    /// 3. Applies changes by pushing commits.
    /// 4. Persists the new state upon successful synchronization.
    async fn run(&mut self) -> SyncResult<String> {
        println!("Starting synchronization...");

        // Phase 1: Fetch updates from all remotes to ensure we have the latest data.
        self.fetch_repo("source", &self.source_repo).await?;
        self.fetch_repo("target", &self.target_repo).await?;

        // Phase 2: Analyze divergence by finding heads and the sync base.
        let source_head = self
            .source_repo
            .find_branch("main", git2::BranchType::Local)?
            .get()
            .peel_to_commit()?
            .id();
        let target_head = self
            .target_repo
            .find_branch("main", git2::BranchType::Local)?
            .get()
            .peel_to_commit()?
            .id();

        let base_oid = self.find_sync_base(source_head, target_head)?;

        let (source_ahead, _) = self
            .source_repo
            .graph_ahead_behind(source_head, base_oid)?;
        let (target_ahead, _) = self
            .target_repo
            .graph_ahead_behind(target_head, base_oid)?;

        println!("Analysis complete:");
        println!("- Source is {} commits ahead.", source_ahead);
        println!("- Target is {} commits ahead.", target_ahead);

        // Phase 3: Apply changes and update state.
        let result_message = if source_ahead > 0 && target_ahead > 0 {
            // DIVERGENCE: This is a critical failure case. The safest action is
            // to stop and inform the user to avoid data loss.
            return Err("Repositories have diverged! Manual intervention required.".into());
        } else if source_ahead > 0 {
            println!("Applying {} commits from source to target...", source_ahead);
            /* BEGIN CHANGE: Implement git push from source to target. */
            self.push_commits(&self.source_repo, &self.target_path, "main")?;

            // Upon successful application, update state to the new head.
            self.state.last_source_synced_oid = Some(source_head.to_string());
            self.state.last_target_synced_oid = Some(source_head.to_string()); // Target is now at source_head
            self.save_state()?; // Persist the new state.
            format!("Successfully synchronized {} commit(s) from source to target.", source_ahead)
            /* END CHANGE */
        } else if target_ahead > 0 {
            println!("Applying {} commits from target to source...", target_ahead);
            /* BEGIN CHANGE: Implement git push from target to source. */
            self.push_commits(&self.target_repo, &self.source_path, "main")?;

            // Upon successful application, update state to the new head.
            self.state.last_source_synced_oid = Some(target_head.to_string()); // Source is now at target_head
            self.state.last_target_synced_oid = Some(target_head.to_string());
            self.save_state()?; // Persist the new state.
            format!("Successfully synchronized {} commit(s) from target to source.", target_ahead)
            /* END CHANGE */
        } else {
            "Repositories are already in sync.".to_string()
        };

        Ok(result_message)
    }

    /* BEGIN CHANGE: Add helper function for push logic. */
    /// Pushes changes from a source repository to a target path.
    ///
    /// This function creates a temporary, in-memory remote, configures it to
    /// point to the target repository's path, and then executes a git push.
    fn push_commits(
        &self,
        source_repo: &Repository,
        target_path: &Path,
        branch: &str,
    ) -> SyncResult<()> {
        // Create an "anonymous" remote that exists only in memory for this operation.
        // This is safer than creating and deleting a named remote from the repo config.
        let target_url = target_path.to_str().ok_or("Invalid target path format")?;
        let mut remote = source_repo.remote_create_anonymous(target_url)?;

        // The refspec defines what to push. Here, we push the local branch
        // to the remote's branch of the same name.
        let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);

        // For remote repositories (http/ssh), authentication would be configured here
        // using `RemoteCallbacks`. For local file paths, no credentials are needed.
        let mut push_options = git2::PushOptions::new();
        // Example for token auth:
        // let mut callbacks = git2::RemoteCallbacks::new();
        // callbacks.credentials(|_url, _username, _allowed| {
        //     git2::Cred::userpass_plaintext("YOUR_GIT_TOKEN", "")
        // });
        // push_options.remote_callbacks(callbacks);

        println!("Pushing refspec '{}' to '{}'...", refspec, target_url);
        remote.push(&[refspec], Some(&mut push_options))?;
        println!("Push successful.");

        Ok(())
    }
    /* END CHANGE */

    /// Helper to fetch updates for a given repository from its "origin" remote.
    async fn fetch_repo(&self, name: &str, repo: &Repository) -> SyncResult<()> {
        println!("Fetching updates for {} repository...", name);
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["main"], None, None)?;
        Ok(())
    }

    /// Finds the common base for synchronization.
    ///
    /// The strategy is to first trust our persistent state. If the state is missing
    /// or the commits can't be found (e.g., due to a rebase), it falls back to
    /// calculating a new merge base from the current heads.
    fn find_sync_base(&self, source_oid: Oid, target_oid: Oid) -> SyncResult<Oid> {
        if let (Some(s_oid_str), Some(t_oid_str)) =
            (&self.state.last_source_synced_oid, &self.state.last_target_synced_oid)
        {
            println!("Found previous sync state. Calculating merge base from saved OIDs.");
            let last_source_oid = Oid::from_str(s_oid_str)?;
            let last_target_oid = Oid::from_str(t_oid_str)?;

            return self.source_repo.merge_base(last_source_oid, last_target_oid)
                .map_err(|e| format!("Could not find merge base for saved OIDs {s_oid_str} and {t_oid_str}. Have the branches been rebased? Error: {e}").into());
        }

        // Fallback for the very first run or if state was lost.
        println!("No previous sync state found. Calculating merge base from current heads.");
        self.source_repo
            .merge_base(source_oid, target_oid)
            .map_err(|e| e.into())
    }

    /// Persists the current in-memory state to a JSON file on disk.
    fn save_state(&self) -> SyncResult<()> {
        println!("Saving sync state to disk at {:?}...", self.state_path);
        let state_json = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&self.state_path, state_json)?;
        println!("State saved successfully.");
        Ok(())
    }
}

// --- 4. The Command Orchestrator ---
/// Handles the `sync-run` command dispatched from an external caller (e.g., FFI).
///
/// This function is the public-facing entry point. It parses arguments,
/// instantiates the `SyncEngine`, and executes the main `run` loop.
pub async fn handle_run_sync(args: &[String]) -> Result<String, String> {
    if args.len() != 2 {
        return Err(
            "Usage: sync-run <path_to_source_repo> <path_to_target_repo>".to_string(),
        );
    }

    let mut engine = SyncEngine::new(&args[0], &args[1])
        .map_err(|e| format!("Failed to initialize sync engine: {}", e))?;

    engine.run().await.map_err(|e| e.to_string())
}