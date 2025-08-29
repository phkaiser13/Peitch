/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * platform_posix.c - POSIX-specific implementation of the platform layer.
 *
 * This file provides the concrete implementation of the functions declared in
 * platform.h for POSIX-compliant operating systems like Linux, macOS, and
 * various BSDs. It relies on standard C libraries and POSIX APIs.
 *
 * Unlike Windows, most modern POSIX terminals (like xterm, gnome-terminal,
 * iTerm2) support ANSI escape sequences for TUI rendering out of the box.
 * Therefore, the initialization and cleanup functions in this implementation
 * are often simpler, acting as placeholders to satisfy the interface contract.
 *
 * This file should only be included in the build process when compiling on a
 * non-Windows (POSIX-like) target.
 *
 * SPDX-License-Identifier: Apache-2.0 */

// This preprocessor directive ensures the code is only compiled on non-Windows
// systems, effectively targeting POSIX-compliant environments.
#ifndef _WIN32

#include "platform.h"
#include <stdio.h>
#include <stdlib.h> // For getenv
#include <string.h> // For strncpy

// --- Interface Implementation ---

/**
 * @see platform.h
 */
bool platform_global_init(void) {
    // On POSIX systems, terminals almost universally support ANSI escape
    // sequences by default. No special initialization is required.
    // We provide an empty implementation to fulfill the API contract.
    return true;
}

/**
 * @see platform.h
 */
void platform_global_cleanup(void) {
    // As no resources were acquired in `platform_global_init`,
    // there is nothing to clean up. This function is a no-op.
}

/**
 * @see platform.h
 */
void platform_clear_screen(void) {
    // Use the standard ANSI escape sequence to clear the screen and reset
    // the cursor position. This is the same code used in the Windows
    // implementation after enabling virtual terminal processing, demonstrating
    // the effectiveness of the platform abstraction.
    // \x1B[2J clears the entire screen.
    // \x1B[H moves the cursor to the home position (top-left).
    printf("\x1B[2J\x1B[H");
    fflush(stdout); // Ensure the command is sent to the terminal immediately.
}

/**
 * @see platform.h
 */
bool platform_get_home_dir(char* buffer, size_t buffer_size) {
    if (buffer == NULL || buffer_size == 0) {
        return false;
    }

    // On POSIX systems, the "HOME" environment variable is the standard
    // way to locate the user's home directory.
    const char* home_dir = getenv("HOME");

    if (home_dir == NULL) {
        // The HOME variable is not set, which is highly unusual but possible.
        return false;
    }

    // Use strncpy for a safe copy, ensuring we don't write past the end
    // of the provided buffer.
    strncpy(buffer, home_dir, buffer_size - 1);
    buffer[buffer_size - 1] = '\0'; // Manually null-terminate.

    // Check for truncation. If the source string was as long or longer than
    // the buffer, strncpy does not guarantee null-termination. Our manual
    // termination handles this, but we should ideally signal that the full
    // path might not have fit. For now, we check if the length is less than
    // the buffer size.
    return strlen(home_dir) < buffer_size;
}

#endif // !_WIN32