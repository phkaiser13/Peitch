/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: policy_handler.h
*
* This header file defines the public interface for the 'policy' command
* group handler. It declares the main entry point function,
* `handle_policy_command`, which is responsible for parsing and executing all
* subcommands related to Policy-as-Code operations, such as scanning
* Kubernetes manifests against a set of rules (e.g., 'ph policy scan').
* The main CLI dispatcher (`cli_parser.c`) uses this function to delegate
* control for any command prefixed with 'policy'.
*
* SPDX-License-Identifier: Apache-2.0 */

#ifndef POLICY_HANDLER_H
#define POLICY_HANDLER_H

// Include the core API header to get access to the standard status codes
// used throughout the application, such as phStatus and its variants.
#include "ipc/include/ph_core_api.h"

// Use C linkage for C++ compatibility. This prevents the C++ compiler
// from mangling the function name, ensuring that the C linker can find it.
#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Main entry point for handling 'policy' command group subcommands.
 *
 * This function acts as a sub-dispatcher for commands like 'ph policy <subcommand>'.
 * It parses the subcommand provided in argv[0] (e.g., "scan", "apply", "test")
 * and delegates to the appropriate implementation function within policy_handler.c.
 *
 * @param argc The number of arguments in the argv array. This count starts
 *             from the subcommand itself.
 * @param argv An array of string arguments, where argv[0] is the subcommand
 *             and subsequent elements are its parameters.
 * @return A phStatus code indicating the outcome of the operation.
 *         Returns ph_SUCCESS on successful execution, or a specific error
 *         code (e.g., ph_ERROR_INVALID_ARGS, ph_ERROR_NOT_FOUND) on failure.
 */
phStatus handle_policy_command(int argc, const char** argv);

#ifdef __cplusplus
}
#endif

#endif // POLICY_HANDLER_H