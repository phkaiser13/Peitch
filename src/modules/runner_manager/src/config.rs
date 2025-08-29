/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/runner_manager/src/config.rs
*
* This file defines the data structures for the runner_manager module.
*
* SPDX-License-Identifier: Apache-2.0 */

use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "action")]
pub enum RunnerPayload {
    #[serde(rename = "scale")]
    Scale(ScaleConfig),
    #[serde(rename = "hpa_install")]
    HpaInstall(HpaInstallConfig),
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScaleConfig {
    pub min: u32,
    pub max: u32,
    pub cluster: String,
    pub metric: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HpaInstallConfig {
    pub namespace: String,
    pub metric: String,
    pub target: u32,
}
