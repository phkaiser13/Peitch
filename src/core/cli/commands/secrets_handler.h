/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: secrets_handler.h
*
* This header file defines the public interface for the 'secrets' command
* group handler. It declares the main entry point function,
* `handle_secrets_command`, which is responsible for parsing and executing all
* subcommands related to managing sensitive information, such as API keys,
* tokens, and other credentials (e.g., 'ph secrets get', 'ph secrets set').
* The main CLI dispatcher (`cli_parser.c`) uses this function to delegate
* control for any command prefixed with 'secrets'.
*
* SPDX-License-Identifier: Apache-2.0 */

#ifndef SECRETS_HANDLER_H
#define SECRETS_HANDLER_H

// Include the core API header to get access to the standard status codes
// used throughout the application, such as phStatus and its variants.
#include "ipc/include/ph_core_api.h"

// Use C linkage for C++ compatibility. This prevents the C++ compiler
// from mangling the function name, ensuring that the C linker can find it.
#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Main entry point for handling 'secrets' command group subcommands.
 *
 * This function acts as a sub-dispatcher for commands like 'ph secrets <subcommand>'.
 * It parses the subcommand provided in argv[0] (e.g., "get", "set", "list")
 * and delegates to the appropriate implementation function within secrets_handler.c.
 *
 * @param argc The number of arguments in the argv array. This count starts
 *             from the subcommand itself. For example, in 'ph secrets get my-key',
 *             argc would be 2 and argv would be {"get", "my-key"}.
 * @param argv An array of string arguments, where argv[0] is the subcommand
 *             and subsequent elements are its parameters.
 * @return A phStatus code indicating the outcome of the operation.
 *         Returns ph_SUCCESS on successful execution, or a specific error
 *         code (e.g., ph_ERROR_INVALID_ARGS, ph_ERROR_NOT_FOUND) on failure.
 */
phStatus handle_secrets_command(int argc, const char** argv);

#ifdef __cplusplus
}
#endif

#endif // SECRETS_HANDLER_H