/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * issue_service.rs - Abstracted business logic for issue tracker services.
 *
 * This file implements a sophisticated, trait-based architecture for interacting
 * with multiple issue tracking services. It defines a generic `Issue` data
 * model and an `IssueTrackerService` trait that acts as a contract for any
 * concrete service implementation.
 *
 * This design decouples the command logic from the specifics of any single
 * API (like GitHub's), making the module highly extensible and maintainable.
 *
 * SPDX-License-Identifier: Apache-2.0 */

use serde::Deserialize;

// --- 1. The Generic Data Model ---
// This is our canonical representation of an issue, independent of the source.

#[derive(Debug)]
pub struct Issue {
    pub id: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub url: String,
}

// --- 2. The Service Contract (Trait) ---

// A custom error type for our services.
type ServiceError = Box<dyn std::error::Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

#[async_trait::async_trait]
pub trait IssueTrackerService {
    /// Fetches details for a single issue by its ID.
    async fn get_issue_details(&self, repo: &str, issue_id: &str) -> ServiceResult<Issue>;
}

// --- 3. The Concrete Implementation for GitHub ---

// Structs that precisely match the JSON response from the GitHub API.
// `serde` will use these to automatically parse the JSON.
#[derive(Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Deserialize)]
struct GitHubApiResponse {
    number: u64,
    title: String,
    html_url: String,
    state: String,
    user: GitHubUser,
}

pub struct GitHubApiService {
    client: reqwest::Client,
}

impl GitHubApiService {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl IssueTrackerService for GitHubApiService {
    async fn get_issue_details(&self, repo: &str, issue_id: &str) -> ServiceResult<Issue> {
        let url = format!("https://api.github.com/repos/{}/issues/{}", repo, issue_id);

        let response = self.client
            .get(&url)
            // It's good practice to set a User-Agent.
            .header("User-Agent", "ph-issue-tracker-module/1.0")
            .send()
            .await?
            .error_for_status()?; // This will error on 4xx/5xx responses.

        let api_response: GitHubApiResponse = response.json().await?;

        // **The crucial mapping step** from the specific API response
        // to our generic, canonical `Issue` model.
        let issue = Issue {
            id: api_response.number,
            title: api_response.title,
            author: api_response.user.login,
            state: api_response.state,
            url: api_response.html_url,
        };

        Ok(issue)
    }
}

// --- 4. The Command Orchestrator ---

/// Handles the `issue-get` command dispatched from the FFI layer.
pub async fn handle_get_issue(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Usage: issue-get <user/repo> <issue_id>".to_string());
    }
    let repo_path = &args[0];
    let issue_id = &args[1];

    // For now, we hardcode the GitHub service. In the future, this could be
    // selected based on configuration.
    let service: Box<dyn IssueTrackerService> = Box::new(GitHubApiService::new());

    println!("Fetching issue {} from {}...", issue_id, repo_path);

    match service.get_issue_details(repo_path, issue_id).await {
        Ok(issue) => {
            // Display the generic `Issue` model to the user.
            println!("\n--- Issue Details ---");
            println!("  ID:      #{}", issue.id);
            println!("  Title:   {}", issue.title);
            println!("  Author:  {}", issue.author);
            println!("  State:   {}", issue.state);
            println!("  URL:     {}", issue.url);
            println!("---------------------");
            Ok(())
        }
        Err(e) => Err(format!("Failed to fetch issue details: {}", e)),
    }
}