/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * tui.h - Interface for the Text-based User Interface.
 *
 * This header defines the functions for managing the interactive console
 * interface of ph. The TUI is the primary mode of operation when the user
 * runs the application without any command-line arguments.
 *
 * Its responsibilities include:
 * - Displaying the main menu of available commands.
 * - Prompting the user for input and reading their choice.
 * - Displaying formatted output, errors, and confirmation messages.
 * - Orchestrating the interactive session loop.
 *
 * The TUI module relies on the platform abstraction layer for screen
 * manipulation and the module loader to dynamically build its menu options.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef TUI_H
#define TUI_H

#include <stdbool.h> // Para o tipo bool
#include <stddef.h>  // Para o tipo size_t

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Displays the main menu and enters the interactive loop.
 *
 * This is the main entry point for the TUI module. It clears the screen,
 * prints a header, and lists all the primary commands available to the user.
 * It then waits for user input and dispatches the corresponding action until
 * the user chooses to exit.
 */
void tui_show_main_menu(void);

/**
 * @brief Displays a formatted error message to the user in a consistent style.
 *
 * @param message The error message to display.
 */
void tui_print_error(const char* message);

/**
 * @brief Displays a formatted success message to the user.
 *
 * @param message The success message to display.
 */
void tui_print_success(const char* message);

/**
 * @brief Prompts the user for input with a specific message.
 *
 * This function displays a prompt and waits for the user to enter a line of
 * text, which is then returned to the caller.
 *
 * @param prompt The message to display to the user (e.g., "Enter commit message:").
 * @param buffer A character buffer to store the user's input.
 * @param buffer_size The size of the `buffer`.
 * @return true if input was received, false on error or end-of-file.
 */
bool tui_prompt_user(const char* prompt, char* buffer, size_t buffer_size);


#ifdef __cplusplus
} // extern "C"
#endif

#endif // TUI_H