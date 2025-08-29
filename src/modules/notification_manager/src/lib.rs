/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/notification_manager/src/lib.rs
 *
 * This module provides a centralized way to send notifications to various
 * services like Slack and to create issues in issue trackers like GitHub.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{Context, Result};
use serde::Serialize;

// --- Public Data Structures ---

pub struct SlackNotification<'a> {
    pub webhook_url: &'a str,
    pub message: &'a str,
}

pub struct IssueNotification<'a> {
    pub repo: &'a str, // e.g., "owner/repo"
    pub title: &'a str,
    pub body: &'a str,
}

// --- Public Function ---

/// Sends notifications to the specified services.
pub async fn send_notification(
    slack_payload: Option<SlackNotification<'_>>,
    issue_payload: Option<IssueNotification<'_>>,
) -> Result<()> {
    if let Some(slack) = slack_payload {
        send_slack_message(slack.webhook_url, slack.message).await?;
    }

    if let Some(issue) = issue_payload {
        let created_issue = issue_tracker::create_issue(issue.repo, issue.title, issue.body)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        println!("[notification_manager] Successfully created issue #{}: {}", created_issue.id, created_issue.url);
    }

    Ok(())
}


// --- Private Helpers ---

#[derive(Serialize)]
struct SlackMessage<'a> {
    text: &'a str,
}

async fn send_slack_message(webhook_url: &str, message: &str) -> Result<()> {
    println!("[notification_manager] Sending Slack message...");
    let client = reqwest::Client::new();
    let payload = SlackMessage { text: message };

    client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .context("Failed to send request to Slack webhook")?
        .error_for_status()
        .context("Slack webhook returned an error status")?;

    println!("[notification_manager] Slack message sent successfully.");
    Ok(())
}
