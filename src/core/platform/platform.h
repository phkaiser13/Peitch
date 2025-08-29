/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * platform.h - Platform Abstraction Layer Interface.
 *
 * This header defines a consistent interface for interacting with
 * operating system-specific functionalities. The goal is to isolate all
 * platform-dependent code within the `platform_win.c` and `platform_posix.c`
 * implementation files, allowing the rest of the core application to be
 * written in a portable, OS-agnostic manner.
 *
 * Functions declared here handle tasks such as:
 * - Initializing the console for proper rendering (e.g., enabling ANSI
 *   escape codes on Windows).
 * - Clearing the terminal screen.
 * - Retrieving environment-specific paths (e.g., user's home directory).
 * - Abstracting file system path separators.
 * - Providing the correct file extension for shared libraries (.dll vs .so).
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef PLATFORM_H
#define PLATFORM_H

#include <stddef.h> // For size_t
#include <stdbool.h> // For bool type

#ifdef __cplusplus
extern "C" {
#endif

/* Define platform-specific macros for convenience */
#ifdef _WIN32
    #define PLATFORM_WINDOWS
    #define PATH_SEPARATOR '\\'
    #define MODULE_EXTENSION ".dll"
#else
    #define PLATFORM_POSIX
    #define PATH_SEPARATOR '/'
    #define MODULE_EXTENSION ".so"
#endif


/**
 * @brief Performs one-time global initialization for the platform.
 *
 * This function should be called once at the very beginning of the application's
 * lifecycle. On Windows, this is critical for enabling virtual terminal
 * sequences, which allows the use of ANSI escape codes for colors and cursor
 * control in `cmd.exe` and PowerShell. On POSIX systems, this function
 * may have no effect as terminals typically support these features by default.
 *
 * @return true on success, false on failure.
 */
bool platform_global_init(void);

/**
 * @brief Performs one-time global cleanup for the platform.
 *
 * This function should be called just before the application exits to release
 * any resources acquired by `platform_global_init`.
 */
void platform_global_cleanup(void);

/**
 * @brief Clears the console screen.
 *
 * This function sends the appropriate command or sequence of characters to
 * the console to clear its contents and move the cursor to the top-left
 * position.
 */
void platform_clear_screen(void);

/**
 * @brief Safely retrieves the path to the user's home directory.
 *
 * This function abstracts the process of finding the home directory, which
 * varies between operating systems (e.g., using %USERPROFILE% on Windows,
 * $HOME on POSIX).
 *
 * @param buffer A pointer to the character buffer where the path will be stored.
 * @param buffer_size The total size of the `buffer`.
 * @return true if the path was successfully retrieved and fits in the buffer,
 *         false otherwise.
 */
bool platform_get_home_dir(char* buffer, size_t buffer_size);


#ifdef __cplusplus
} // extern "C"
#endif

#endif // PLATFORM_H