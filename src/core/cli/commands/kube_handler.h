/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/core/cli/commands/kube_handler.h
 *
 * This header file defines the public interface for the Kubernetes command
 * handler. Its primary purpose is to declare the main entry function that
 * the central CLI parser will call when it identifies a command prefixed with
 * "kube". This function acts as a router, delegating the specific subcommand
 * (like "sync", "drift", or "rollout") to the appropriate parsing and execution
 * logic within the corresponding .c file. By exposing only this single entry
 * point, we encapsulate the complexity of handling all Kubernetes-related
 * subcommands and maintain a clean separation of concerns.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#ifndef KUBE_HANDLER_H
#define KUBE_HANDLER_H

/**
 * @brief Handles all "kube" subcommands by parsing arguments and dispatching
 *        to the correct Rust FFI module.
 *
 * This function is the main entry point for any command that starts with "ph kube".
 * It inspects the first argument in `argv` to determine the specific subcommand
 * (e.g., "sync", "drift", "rollout"). Based on the subcommand, it parses the
 * remaining arguments and flags, constructs a JSON payload, and invokes the
 * corresponding function in a dynamically loaded Rust module.
 *
 * @param argc The argument count, including the subcommand. For example, for
 *             "ph kube sync --cluster dev", argc would be 3, and argv would
 *             contain ["sync", "--cluster", "dev"].
 * @param argv An array of strings representing the subcommand and its arguments.
 * @return Returns 0 on success, or a non-zero error code on failure. The specific
 *         error codes are determined by the underlying Rust modules or parsing
 *         logic.
 */
int handle_kube_command(int argc, const char** argv);

#endif // KUBE_HANDLER_H