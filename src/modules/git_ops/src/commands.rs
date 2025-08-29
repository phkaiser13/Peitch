/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* commands.rs - Business logic for git_ops module commands.
*
* This module implements the high-level logic for the commands supported by
* the `git_ops` module. It orchestrates calls to the `git_wrapper` to perform
* intelligent, multi-step Git operations.
*
* The refactored `handle_send` command is a prime example of this module's
* purpose. Instead of naively running `git push origin main`, it dynamically
* discovers the current branch's upstream remote and branch, asks for user
* confirmation, and only then proceeds. This makes the tool safer and more
* adaptable to real-world developer workflows.
*
* Error handling is made more robust through the `CommandError` enum, which
* allows the module to return specific error states to the core, rather than
* just generic failure messages.
*
* SPDX-License-Identifier: Apache-2.0 */

use crate::git_wrapper; // Import our git wrapper module
use crate::log_to_core; // Import the logging helper from lib.rs
use crate::phLogLevel;
use std::io::{self, Write};

/// Defines specific, structured errors for command logic.
#[derive(Debug, PartialEq)] // PartialEq for easier testing
pub enum CommandError {
    /// Wraps a generic error from the underlying git command.
    GitError(String),
    /// Indicates the user tried to send changes, but the working tree was clean.
    NoChanges,
    /// The current branch does not have a configured upstream remote/branch.
    NoUpstreamConfigured,
    /// The user did not provide a required commit message.
    MissingCommitMessage,
    /// The user aborted the operation at a confirmation prompt.
    OperationAborted,
}

// Implement the From trait to easily convert GitResult errors into our CommandError.
// This allows us to use the `?` operator on functions returning GitResult.
impl From<String> for CommandError {
    fn from(err: String) -> Self {
        CommandError::GitError(err)
    }
}

/// A specialized Result type for our command functions.
type CommandResult = Result<String, CommandError>;

/// Handles the 'status' command.
///
/// # Arguments
/// * `repo_path` - An optional path to the repository. If None, operates on the current directory.
pub fn handle_status(repo_path: Option<&str>) -> CommandResult {
    log_to_core(phLogLevel::Info, "Handling 'status' command.");

    match git_wrapper::git_status(repo_path) {
        Ok(output) => {
            if output.is_empty() {
                Ok("Working tree clean. Nothing to commit.".to_string())
            } else {
                let message = format!(
                    "Changes to be committed or untracked files:\n{}",
                    output
                );
                Ok(message)
            }
        }
        Err(e) => Err(CommandError::GitError(e)),
    }
}

/// Handles the 'SND' command: stages, commits, and pushes changes intelligently.
///
/// # Arguments
/// * `repo_path` - An optional path to the repository. If None, operates on the current directory.
/// * `args` - A slice of strings containing command arguments. The commit message
///   is expected to be constructed from these arguments.
/// * `skip_confirmation` - A flag to bypass the interactive push confirmation,
///   primarily used for testing.
pub fn handle_send(
    repo_path: Option<&str>,
    args: &[String],
    skip_confirmation: bool,
) -> CommandResult {
    log_to_core(phLogLevel::Info, "Handling 'SND' command.");

    // 1. Get commit message from arguments.
    if args.len() <= 1 {
        return Err(CommandError::MissingCommitMessage);
    }
    let commit_message = args[1..].join(" ");

    // 2. Stage all changes.
    log_to_core(phLogLevel::Debug, "Staging all changes.");
    git_wrapper::git_add(repo_path, ".")?;
    log_to_core(phLogLevel::Info, "Files staged successfully.");

    // 3. Commit the changes.
    log_to_core(
        phLogLevel::Debug,
        &format!("Committing with message: '{}'", commit_message),
    );
    match git_wrapper::git_commit(repo_path, &commit_message) {
        Ok(_) => log_to_core(phLogLevel::Info, "Changes committed successfully."),
        // Handle cases where there's nothing to commit. Git can return different messages.
        Err(e) if e.contains("nothing to commit") || e.contains("no changes added to commit") => {
            log_to_core(
                phLogLevel::Info,
                "Working tree clean. No new commit created.",
            );
            return Err(CommandError::NoChanges);
        }
        Err(e) => return Err(CommandError::GitError(e)),
    };

    // 4. Dynamically determine remote and branch for push.
    let upstream_str = git_wrapper::git_get_upstream_branch(repo_path).map_err(|_| {
        log_to_core(
            phLogLevel::Error,
            "Could not determine upstream branch. Please set it with 'git push -u <remote> <branch>'.",
        );
        CommandError::NoUpstreamConfigured
    })?;

    let (remote, branch) = upstream_str.split_once('/').ok_or_else(|| {
        CommandError::GitError(format!("Could not parse upstream: '{}'", upstream_str))
    })?;

    // 5. Ask for user confirmation before pushing (if not skipped).
    if !skip_confirmation {
        print!("Push to remote '{}/{}'? (y/N): ", remote, branch);
        io::stdout().flush().unwrap(); // Ensure the prompt is shown before input
        let mut confirmation = String::new();
        io::stdin().read_line(&mut confirmation).unwrap();
        if confirmation.trim().to_lowercase() != "y" {
            return Err(CommandError::OperationAborted);
        }
    }

    // 6. Push to the dynamically determined remote and branch.
    log_to_core(
        phLogLevel::Debug,
        &format!("Pushing to {} {}", remote, branch),
    );
    git_wrapper::git_push(repo_path, remote, branch)?;
    log_to_core(phLogLevel::Info, "Successfully pushed to remote.");

    Ok(format!(
        "Successfully sent changes to {}/{}",
        remote, branch
    ))
}

// --- Unit and Integration Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Output};
    use tempfile::TempDir;

    /// A helper structure to manage temporary git repositories for testing.
    /// The TempDir will automatically clean up the directories on drop.
    struct TestRepo {
        local_dir: TempDir,
        // The remote is also a TempDir to ensure it's cleaned up.
        _remote_dir: TempDir,
        remote_path: String,
    }

    /// Helper to run a command in a specific directory and panic on failure.
    fn run_command_in_dir(dir: &Path, args: &[&str]) -> Output {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("Failed to execute git command");

        assert!(
            output.status.success(),
            "Git command failed: git {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// Sets up a complete test environment with a local and a bare remote repository.
    /// This is the foundation for most integration tests.
    fn setup_test_repo_with_remote() -> TestRepo {
        // 1. Create a directory for the bare remote repo.
        let remote_dir = TempDir::new().expect("Failed to create remote temp dir");
        run_command_in_dir(remote_dir.path(), &["init", "--bare"]);
        let remote_path = remote_dir
            .path()
            .to_str()
            .expect("Path is not valid UTF-8")
            .to_string();

        // 2. Create the local repository.
        let local_dir = TempDir::new().expect("Failed to create local temp dir");
        let local_path = local_dir.path();

        // 3. Init and configure the local repo.
        run_command_in_dir(local_path, &["init", "-b", "main"]);
        run_command_in_dir(local_path, &["config", "user.name", "Test User"]);
        run_command_in_dir(local_path, &["config", "user.email", "test@example.com"]);

        // 4. Add the bare repo as a remote named "origin".
        run_command_in_dir(local_path, &["remote", "add", "origin", &remote_path]);

        TestRepo {
            local_dir,
            _remote_dir: remote_dir,
            remote_path,
        }
    }

    #[test]
    fn test_handle_send_happy_path() {
        // Arrange: Set up a repo with a remote and an initial commit that tracks it.
        let repo = setup_test_repo_with_remote();
        let local_path = repo.local_dir.path();
        let local_path_str = local_path.to_str().unwrap();

        // Create and push an initial commit to set upstream tracking.
        fs::write(local_path.join("initial.txt"), "base").unwrap();
        run_command_in_dir(local_path, &["add", "."]);
        run_command_in_dir(local_path, &["commit", "-m", "Initial commit"]);
        run_command_in_dir(local_path, &["push", "-u", "origin", "main"]);

        // Make a new change to be sent by our function.
        fs::write(local_path.join("new_file.txt"), "hello").unwrap();

        // Act: Run handle_send with the explicit path, a commit message, and skip confirmation.
        let args = vec!["SND".to_string(), "feat: Add new file".to_string()];
        let result = handle_send(Some(local_path_str), &args, true);

        // Assert: The command should succeed.
        assert!(result.is_ok(), "Expected Ok, got Err({:?})", result.err());
        assert_eq!(
            result.unwrap(),
            "Successfully sent changes to origin/main"
        );

        // Further assert that the commit exists on the remote.
        let log_output = git_wrapper::execute_git_command(
            Some(&repo.remote_path),
            &["log", "-1", "--oneline"],
            None,
        )
            .unwrap();
        assert!(log_output.contains("feat: Add new file"));
    }

    #[test]
    fn test_handle_send_no_changes() {
        // Arrange: Set up a clean repo that is already tracking a remote.
        let repo = setup_test_repo_with_remote();
        let local_path = repo.local_dir.path();
        let local_path_str = local_path.to_str().unwrap();

        // Create and push an initial commit so the repo is not empty.
        fs::write(local_path.join("file.txt"), "content").unwrap();
        run_command_in_dir(local_path, &["add", "."]);
        run_command_in_dir(local_path, &["commit", "-m", "Initial"]);
        run_command_in_dir(local_path, &["push", "-u", "origin", "main"]);

        // Act: Run the send function with no new changes in the working directory.
        let args = vec!["SND".to_string(), "some message".to_string()];
        let result = handle_send(Some(local_path_str), &args, true);

        // Assert: The result should be the specific NoChanges error.
        assert_eq!(result, Err(CommandError::NoChanges));
    }

    #[test]
    fn test_handle_send_no_upstream_configured() {
        // Arrange: Set up a repo, make a commit, but DO NOT connect it to a remote or push.
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path();
        let local_path_str = local_path.to_str().unwrap();

        run_command_in_dir(local_path, &["init", "-b", "main"]);
        run_command_in_dir(local_path, &["config", "user.name", "Test"]);
        run_command_in_dir(local_path, &["config", "user.email", "a@b.c"]);
        fs::write(local_path.join("file.txt"), "content").unwrap();
        run_command_in_dir(local_path, &["add", "."]);
        run_command_in_dir(local_path, &["commit", "-m", "A commit"]);

        // Act: Try to send changes.
        let args = vec!["SND".to_string(), "some message".to_string()];
        let result = handle_send(Some(local_path_str), &args, true);

        // Assert: It should fail because no upstream is configured.
        assert_eq!(result, Err(CommandError::NoUpstreamConfigured));
    }

    #[test]
    fn test_handle_send_missing_commit_message() {
        // Arrange: Only the command name is provided. No repo needed for this validation.
        let args = vec!["SND".to_string()];

        // Act
        let result = handle_send(None, &args, true);

        // Assert
        assert_eq!(result, Err(CommandError::MissingCommitMessage));
    }
}