// Copyright (C) 2025 Pedro Henrique / phkaiser13
//
// File: src/modules/domains/kubernetes/cli/registration.c
//
// This file implements the command registration for the Kubernetes domain.
// It acts as the bridge that allows the domain-specific 'local' command
// to be visible to the generic core CLI dispatcher without creating a
// circular dependency.
//
// SPDX-License-Identifier: Apache-2.0

#include "core/cli/cli_parser.h"
#include "local_handler.h"
#include "registration.h"

void k8s_cli_register_commands(void) {
    // Call the core registration function to register the "local" command
    // and associate it with its handler.
    cli_register_command_group("local", handle_local_command);
}
