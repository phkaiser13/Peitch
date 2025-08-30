/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * installer.c - Main installation orchestrator for phpkg.
 * 
 * This module orchestrates the entire package installation process by
 * coordinating between the detector, resolver, and individual package
 * manager wrappers. It implements the intelligent fallback system,
 * handles user interactions, manages environment variables, and ensures
 * smooth installation across different platforms and package managers.
 * 
 * SPDX-License-Identifier: Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <stdarg.h>
#include <time.h>

#ifdef _WIN32
    #include <windows.h>
    #include <io.h>
    #define isatty _isatty
    #define fileno _fileno
#else
    #include <unistd.h>
    #include <sys/wait.h>
    #include <signal.h>
#endif

/* Include other modules */
#include "detector.h"
#include "resolver.h"
#include "env_manager.h"

/* Installation options */
typedef struct {
    bool force;              /* Force installation even if exists */
    bool yes;                /* Auto-yes to prompts */
    bool quiet;              /* Minimal output */
    bool verbose;            /* Detailed output */
    bool offline;            /* Use offline cache only */
    bool global;             /* System-wide installation */
    char* manager;           /* Forced package manager */
    char* version;           /* Specific version */
    int timeout;             /* Command timeout in seconds */
} InstallOptions;

/* Installation result */
typedef struct {
    bool success;
    char package_name[128];
    char manager_used[64];
    char version_installed[64];
    char error_message[512];
    time_t install_time;
    int exit_code;
} InstallResult;

/* Command execution structure */
typedef struct {
    char command[1024];
    char output[4096];
    int exit_code;
    bool timed_out;
} CommandResult;

/* Color output support */
typedef enum {
    COLOR_RESET,
    COLOR_RED,
    COLOR_GREEN,
    COLOR_YELLOW,
    COLOR_BLUE,
    COLOR_MAGENTA,
    COLOR_CYAN,
    COLOR_WHITE
} Color;

/* Global state */
static bool g_color_enabled = true;
static InstallOptions g_default_options = {
    .force = false,
    .yes = false,
    .quiet = false,
    .verbose = false,
    .offline = false,
    .global = false,
    .manager = NULL,
    .version = NULL