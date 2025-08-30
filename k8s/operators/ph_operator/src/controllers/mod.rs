/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: mod.rs
 *
 * This file serves as the module declaration hub for all controllers within the
 * operator. In Rust's module system, a `mod.rs` file declares the sub-modules
 * contained within its parent directory. This allows for a clean separation of
 * concerns, where each controller's logic is encapsulated in its own file but
 * is made accessible to the rest of the application (e.g., `main.rs`) through
 * this central point.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

// Declare the sub-modules. This makes `pipeline_controller`,
// `preview_controller`, and `release_controller` available under the
// `crate::controllers` path.

pub mod pipeline_controller;
pub mod preview_controller;
pub mod release_controller;
pub mod utils;
pub mod metrics_analyzer; 
pub mod autoheal_controller;
pub mod dr_controller;