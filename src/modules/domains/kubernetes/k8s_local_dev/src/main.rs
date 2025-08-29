/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/k8s_local_dev/src/main.rs
 *
 * FFI entrypoint for the k8s_local_dev module.
 * Exposes `run_local_dev(argc, argv)` which parses args via clap (from cli.rs),
 * dispatches to provisioners (via provisioners::get_provisioner) or profile_applier,
 * and executes async tasks on a Tokio runtime.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{Context, Result};
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;

mod cli;
mod provisioners;
mod profile_applier;
mod sync;

/// The top-level async runner that does the real work.
///
/// It accepts the vector of arguments (as Strings) — this is suitable for using
/// `Cli::parse_from(args)` so clap doesn't read from process args directly.
async fn run_async_logic(args: Vec<String>) -> Result<()> {
    // Parse CLI from provided args (first element should normally be program name).
    let cli = cli::Cli::parse_from(args);

    match cli.command {
        cli::Commands::Create(create_args) => {
            let name = create_args.cluster_name;
            let provider = create_args.provider;
            let k8s_version = create_args.k8s_version;
            println!("➡️  Creating cluster '{}' using provider {:?} (k8s_version={})", name, provider, k8s_version);

            // Factory returns boxed trait object implementing Provisioner
            let provisioner = provisioners::get_provisioner(provider)
                .with_context(|| format!("Unsupported provider for create: {:?}", provider))?;

            provisioner
                .create(&name, &k8s_version)
                .await
                .with_context(|| format!("Failed to create cluster '{}' with provider {:?}", name, provider))?;

            println!("✅ Cluster '{}' created (provider: {:?})", name, provider);
        }

        cli::Commands::Delete(delete_args) => {
            let name = delete_args.cluster_name;
            let provider = delete_args.provider;
            println!("➡️  Deleting cluster '{}' using provider {:?}", name, provider);

            let provisioner = provisioners::get_provisioner(provider)
                .with_context(|| format!("Unsupported provider for delete: {:?}", provider))?;

            provisioner
                .delete(&name)
                .await
                .with_context(|| format!("Failed to delete cluster '{}' with provider {:?}", name, provider))?;

            println!("✅ Cluster '{}' deleted (provider: {:?})", name, provider);
        }

        cli::Commands::List(list_args) => {
            let provider = list_args.provider;
            println!("➡️  Listing clusters for provider {:?}", provider);

            let provisioner = provisioners::get_provisioner(provider)
                .with_context(|| format!("Unsupported provider for list: {:?}", provider))?;

            provisioner
                .list()
                .await
                .with_context(|| format!("Failed to list clusters for provider {:?}", provider))?;
        }

        cli::Commands::ApplyProfile(apply_args) => {
            let profile_path: PathBuf = apply_args.profile_path;
            if let Some(provider) = apply_args.provider {
                // We do not automatically switch kubecontext here; just warn the user.
                println!("ℹ️  apply-profile received --provider {:?}. Ensure your kubectl context points to the target cluster.", provider);
            }

            println!("➡️  Applying profile from: {}", profile_path.display());

            profile_applier::apply_profile(&profile_path)
                .await
                .with_context(|| format!("Failed to apply profile from {}", profile_path.display()))?;

            println!("✅ Profile applied from: {}", profile_path.display());
        }
        
        cli::Commands::Sync(sync_args) => {
            println!("➡️  Starting local sync cycle...");
            sync::handle_sync(sync_args).await?;
            println!("✅ Local sync cycle completed successfully.");
        }
    }

    Ok(())
}

/// FFI-safe entry point called from C.
///
/// # Safety
/// The caller must ensure `argv` points to `argc` valid C strings.
#[no_mangle]
pub extern "C" fn run_local_dev(argc: c_int, argv: *const *const c_char) -> c_int {
    // Convert argv -> Vec<String>
    let args: Vec<String> = unsafe {
        if argv.is_null() || argc < 0 {
            eprintln!("Invalid argc/argv passed to run_local_dev");
            return 1;
        }

        std::slice::from_raw_parts(argv, argc as usize)
            .iter()
            .map(|&ptr| {
                if ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(ptr).to_string_lossy().into_owned()
                }
            })
            .collect()
    };

    // Build a tokio runtime.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create Tokio runtime: {}", e);
            return 1;
        }
    };

    // Run the async logic and handle errors at the FFI boundary.
    let result = rt.block_on(run_async_logic(args));

    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("\n❌ An error occurred: {:#}", e);
            1
        }
    }
}
