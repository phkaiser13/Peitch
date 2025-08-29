/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: health_handler.h
*
* This header file defines the public interface for the health and auto-heal
* command group handler. It declares the main entry point function,
* `handle_health_command`, which is responsible for parsing and executing all
* subcommands related to application health monitoring and automated remediation.
*
* This single handler processes subcommands for both 'ph health' (e.g., 'check')
* and 'ph autoheal' (e.g., 'enable'), as they are functionally related. The
* main CLI dispatcher (`cli_parser.c`) routes both command groups to this handler.
*
* SPDX-License-Identifier: Apache-2.0 */

#ifndef HEALTH_HANDLER_H
#define HEALTH_HANDLER_H

// Include the core API header to get access to the standard status codes
// used throughout the application, such as phStatus and its variants.
#include "ipc/include/ph_core_api.h"

// Use C linkage for C++ compatibility. This prevents the C++ compiler
// from mangling the function name, ensuring that the C linker can find it.
#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Main entry point for handling 'health' and 'autoheal' subcommands.
 *
* This function acts as a sub-dispatcher for commands like 'ph health check'
 * and 'ph autoheal enable'. It parses the subcommand provided in argv[0]
 * and delegates to the appropriate implementation function within health_handler.c.
 *
 * @param argc The number of arguments in the argv array. This count starts
 *             from the subcommand itself.
 * @param argv An array of string arguments, where argv[0] is the subcommand
 *             (e.g., "check", "enable") and subsequent elements are its parameters.
 * @return A phStatus code indicating the outcome of the operation.
 *         Returns ph_SUCCESS on successful execution, or a specific error
 *         code (e.g., ph_ERROR_INVALID_ARGS, ph_ERROR_NOT_FOUND) on failure.
 */
phStatus handle_health_command(int argc, const char** argv);

#ifdef __cplusplus
}
#endif

#endif // HEALTH_HANDLER_H