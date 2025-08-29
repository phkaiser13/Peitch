/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * This file declares the handler for the 'phgit preview' command group.
 * It acts as the entry point for all subcommands related to managing
 * ephemeral preview environments.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#ifndef PREVIEW_HANDLER_H
#define PREVIEW_HANDLER_H

#include "core/platform/platform.h" // For phStatus

/**
 * @brief Handles all 'preview' subcommands.
 *
 * This function is the router for commands like 'phgit preview create',
 * 'phgit preview status', etc. It parses the subcommand and delegates
 * to the appropriate implementation.
 *
 * @param argc The number of arguments for the subcommand.
 * @param argv The argument vector for the subcommand.
 * @return phStatus indicating success or failure.
 */
phStatus handle_preview_command(int argc, const char** argv);

#endif // PREVIEW_HANDLER_H
