/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/config.rs
*
* This file has been refactored to support the various `rollout` actions
* by defining a top-level enum that maps to the JSON sent by the C handler.
*
* SPDX-License-Identifier: Apache-2.0 */

use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum RolloutPayload {
    #[serde(rename = "start")]
    Start(StartConfig),
    #[serde(rename = "status")]
    Status(StatusConfig),
    #[serde(rename = "promote")]
    Promote(IdConfig),
    #[serde(rename = "rollback")]
    Rollback(IdConfig),
    #[serde(rename = "plan")]
    Plan(PlanConfig),
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    pub strategy: String,
    pub app: String,
    pub image: String,
    pub steps: Option<String>,
    pub metric: Option<String>,
    pub analysis_window: Option<String>,
    #[serde(default)]
    pub skip_sig_check: bool,
    pub public_key: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StatusConfig {
    pub id: String,
    pub watch: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IdConfig {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_revision: Option<i32>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlanConfig {
    pub strategy: String,
    pub app: String,
    pub image: String,
    pub preview_url: bool,
}