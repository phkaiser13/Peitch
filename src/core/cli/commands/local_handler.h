/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: local_handler.h
*
* This header file defines the public interface for the 'local' command
* group handler. It declares the main entry point function,
* `handle_local_command`, which is responsible for parsing and executing all
* subcommands related to local, non-cluster operations (e.g., 'ph local run',
* 'ph local config'). The main CLI dispatcher (`cli_parser.c`) uses this
* function to delegate control for any command prefixed with 'local'.
*
* SPDX-License-Identifier: Apache-2.0 */

#ifndef LOCAL_HANDLER_H
#define LOCAL_HANDLER_H

// Include the core API header to get access to the standard status codes
// used throughout the application, such as phStatus and its variants.
#include "ipc/include/ph_core_api.h"

// Use C linkage for C++ compatibility. This prevents the C++ compiler
// from mangling the function name, ensuring that the C linker can find it.
#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Main entry point for handling 'local' command group subcommands.
 *
 * This function acts as a sub-dispatcher for commands like 'ph local <subcommand>'.
 * It parses the subcommand provided in argv[0] and delegates to the
 * appropriate implementation function within local_handler.c.
 *
 * @param argc The number of arguments in the argv array. This count starts
 *             from the subcommand itself. For example, in 'ph local run --fast',
 *             argc would be 2 and argv would be {"run", "--fast"}.
 * @param argv An array of string arguments, where argv[0] is the subcommand
 *             (e.g., "run") and subsequent elements are its parameters.
 * @return A phStatus code indicating the outcome of the operation.
 *         Returns ph_SUCCESS on successful execution, or a specific error
 *         code (e.g., ph_ERROR_INVALID_ARGS, ph_ERROR_NOT_FOUND) on failure.
 */
phStatus handle_local_command(int argc, const char** argv);

#ifdef __cplusplus
}
#endif

#endif // LOCAL_HANDLER_H