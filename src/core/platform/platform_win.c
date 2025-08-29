/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * platform_win.c - Windows-specific implementation of the platform layer.
 *
 * This file provides the concrete implementation of the functions declared in
 * platform.h for the Microsoft Windows operating system. It directly interacts
 * with the Win32 API to handle platform-specific tasks.
 *
 * A key responsibility of this module is the configuration of the Windows
 * Console Host to enable "Virtual Terminal Processing". This allows the
 * console to interpret ANSI escape sequences, which are the standard,
 * cross-platform way to control text color, cursor position, and other
 * advanced terminal features. Without this initialization, any TUI rendering
 * would fail on traditional Windows consoles like cmd.exe.
 *
 * This file should only be included in the build process when compiling on a
 * Windows target.
 *
 * SPDX-License-Identifier: Apache-2.0 */

// This preprocessor directive ensures the code is only compiled on Windows.
#ifdef _WIN32

#include "platform.h"
#include <windows.h>
#include <stdio.h>
#include <stdlib.h> // For getenv_s

// --- Module-level static variables ---

// A handle to the standard output console screen buffer.
// This is required by many Win32 Console API functions.
static HANDLE hConsole = NULL;

// Stores the original console mode so we can restore it on application exit.
// This is good practice to avoid interfering with the user's shell state.
static DWORD dwOriginalOutMode = 0;

// --- Interface Implementation ---

/**
 * @see platform.h
 */
bool platform_global_init(void) {
    // Get a handle to the standard output device.
    hConsole = GetStdHandle(STD_OUTPUT_HANDLE);
    if (hConsole == INVALID_HANDLE_VALUE) {
        // This is a critical failure; we cannot proceed with console manipulation.
        return false;
    }

    // Get the current console mode.
    if (!GetConsoleMode(hConsole, &dwOriginalOutMode)) {
        // If we can't get the mode, we can't set it.
        return false;
    }

    // Enable virtual terminal processing.
    // This flag allows the console to process ANSI escape sequences.
    const DWORD dwRequestedOutModes = ENABLE_VIRTUAL_TERMINAL_PROCESSING;
    const DWORD dwNewMode = dwOriginalOutMode | dwRequestedOutModes;

    if (!SetConsoleMode(hConsole, dwNewMode)) {
        // This can fail on older versions of Windows. The application can
        // still run, but TUI colors and advanced formatting will be disabled.
        // For a modern tool, we consider this a soft failure and log it,
        // but a stricter implementation could treat it as a hard failure.
        // For now, we return true and let the TUI gracefully degrade.
    }

    return true;
}

/**
 * @see platform.h
 */
void platform_global_cleanup(void) {
    // Restore the original console mode to be a good citizen.
    if (hConsole != NULL) {
        SetConsoleMode(hConsole, dwOriginalOutMode);
    }
}

/**
 * @see platform.h
 */
void platform_clear_screen(void) {
    // With ENABLE_VIRTUAL_TERMINAL_PROCESSING, we can use standard ANSI codes.
    // \x1B[2J clears the entire screen.
    // \x1B[H moves the cursor to the home position (top-left).
    // This is preferable to the older, more complex Win32 API method.
    printf("\x1B[2J\x1B[H");
}

/**
 * @see platform.h
 */
bool platform_get_home_dir(char* buffer, size_t buffer_size) {
    if (buffer == NULL || buffer_size == 0) {
        return false;
    }

    // `getenv_s` is a safer alternative to `getenv` on Windows.
    // It checks for buffer size and returns an error code.
    // The "USERPROFILE" environment variable is the standard way to get the
    // user's home directory on Windows.
    size_t required_size;
    errno_t err = getenv_s(&required_size, buffer, buffer_size, "USERPROFILE");

    if (err != 0 || required_size == 0) {
        // If the variable is not set or an error occurs, fail.
        return false;
    }

    return true;
}

#endif // _WIN32