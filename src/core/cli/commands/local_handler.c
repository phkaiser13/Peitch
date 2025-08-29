/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: local_handler.c
*
* This file implements the handler for the 'local' command group. Its primary
* role is to act as a lightweight bridge or shim to a more powerful Rust
* module (`k8s_local_dev`). All complex logic, including argument parsing
* (using the 'clap' library), command execution, and user feedback, is
* handled within the Rust binary.
*
* The `handle_local_command` function receives the arguments from the main
* CLI dispatcher and passes them directly, without modification, to the
* `run_local_dev` function exposed by the Rust module via a Foreign Function
* Interface (FFI). It then translates the integer exit code from the Rust
* world into the application's standard `phStatus` enum for consistent error
* handling across the entire system. This design leverages Rust's safety and
* rich ecosystem for complex tasks while maintaining a simple, stable C ABI.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "local_handler.h"
#include "libs/liblogger/Logger.hpp"
#include "ui/tui.h" // Although not used for output, good practice to include for consistency
#include <stdio.h>
#include <string.h>

// --- Foreign Function Interface (FFI) Declaration ---

/**
 * @brief External function exported by the Rust 'k8s_local_dev' module.
 *
 * This function is the entry point into the Rust logic for all 'local'
 * commands. It is defined in `src/modules/k8s_local_dev/src/main.rs`
 * with a `#[no_mangle]` attribute and `extern "C"` block to ensure a stable C ABI.
 *
 * @param argc The number of arguments.
 * @param argv The argument vector, starting with the subcommand.
 * @return An integer exit code. By convention, 0 indicates success, and any
 *         non-zero value indicates an error.
 */
extern int run_local_dev(int argc, const char** argv);

// --- Public Function Implementation ---

/**
 * @see local_handler.h
 */
phStatus handle_local_command(int argc, const char** argv) {
    // Basic validation to ensure a subcommand was passed from the dispatcher.
    if (argc < 1 || argv[0] == NULL) {
        logger_log(LOG_LEVEL_ERROR, "LocalHandler", "Handler called without a subcommand. This indicates a dispatcher logic error.");
        // This error should ideally not be seen by the user, as the main dispatcher
        // already checks for it. It's a safeguard.
        return ph_ERROR_INVALID_ARGS;
    }

    logger_log_fmt(LOG_LEVEL_INFO, "LocalHandler", "Delegating 'local %s' command and its %d arguments to the Rust FFI bridge.", argv[0], argc - 1);

    // Directly call the external Rust function, passing the arguments as-is.
    // The Rust module contains all the necessary logic to parse subcommands
    // (e.g., 'create-cluster', 'destroy') and their flags using 'clap'.
    int rust_exit_code = run_local_dev(argc, argv);

    // Translate the integer exit code from the Rust module into the
    // application's standard phStatus enum for consistent error handling.
    if (rust_exit_code == 0) {
        logger_log(LOG_LEVEL_INFO, "LocalHandler", "Rust module for 'local' command executed successfully.");
        return ph_SUCCESS;
    } else {
        // The Rust module is expected to have already printed a detailed,
        // user-friendly error message to stderr via its own logging or clap's
        // error reporting. We just log the raw exit code for debugging and
        // return a generic failure status.
        logger_log_fmt(LOG_LEVEL_ERROR, "LocalHandler", "Rust module for 'local' command failed with exit code: %d.", rust_exit_code);
        return ph_ERROR_EXEC_FAILED;
    }
}