/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: runners_handler.h
*
* This header file defines the public interface for the 'runners' command
* group handler. It declares the main entry point function,
* `handle_runners_command`, which is responsible for parsing and executing all
* subcommands related to managing CI/CD runners, such as scaling and
* configuration (e.g., 'ph runners scale', 'ph runners hpa install').
* The main CLI dispatcher (`cli_parser.c`) uses this function to delegate
* control for any command prefixed with 'runners'.
*
* SPDX-License-Identifier: Apache-2.0 */

#ifndef RUNNERS_HANDLER_H
#define RUNNERS_HANDLER_H

// Include the core API header to get access to the standard status codes
// used throughout the application, such as phStatus and its variants.
#include "ipc/include/ph_core_api.h"

// Use C linkage for C++ compatibility. This prevents the C++ compiler
// from mangling the function name, ensuring that the C linker can find it.
#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Main entry point for handling 'runners' command group subcommands.
 *
 * This function acts as a sub-dispatcher for commands like 'ph runners <subcommand>'.
 * It parses the subcommand provided in argv[0] (e.g., "scale", "hpa")
 * and delegates to the appropriate implementation function within runners_handler.c.
 *
 * @param argc The number of arguments in the argv array. This count starts
 *             from the subcommand itself. For example, in 'ph runners scale --min 1',
 *             argc would be 3 and argv would be {"scale", "--min", "1"}.
 * @param argv An array of string arguments, where argv[0] is the subcommand
 *             and subsequent elements are its parameters.
 * @return A phStatus code indicating the outcome of the operation.
 *         Returns ph_SUCCESS on successful execution, or a specific error
 *         code (e.g., ph_ERROR_INVALID_ARGS, ph_ERROR_NOT_FOUND) on failure.
 */
phStatus handle_runners_command(int argc, const char** argv);

#ifdef __cplusplus
}
#endif

#endif // RUNNERS_HANDLER_H