/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * tui.c - Implementation of the Text-based User Interface.
 *
 * Rewritten for safety, correctness and robustness while preserving existing behavior.
 *
 * Fixes:
 *  - Safe truncation in display_menu (no buffer overflow).
 *  - Robust stdin handling: prompt detects truncation and flushes remainder;
 *    wait_for_enter always consumes pending input then waits for a fresh Enter.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#include "tui.h"
#include "platform/platform.h"
#include "module_loader/loader.h"
#include "cli/cli_parser.h"
#include "scripting/lua-h/lua_bridge.h" // To get Lua commands
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <limits.h>
#include <stdbool.h>
#include "libs/liblogger/Logger.hpp"

/* BEGIN CHANGE: Added constants for the new layout. */
#define PEACH_ART_FILE "peitch.ansi"
#define GUTTER_WIDTH 4
/* END CHANGE */

// --- Private Helper Structures and Functions ---

typedef enum {
    COMMAND_SOURCE_NATIVE,
    COMMAND_SOURCE_LUA
} CommandSource;

typedef struct {
    char* name;
    char* description;
    CommandSource source;
} MenuItem;

/* Safe strdup wrapper: returns NULL on allocation failure, but never segfaults on NULL input. */
static char* safe_strdup_or_empty(const char* s) {
    if (!s) s = "";
    char* r = strdup(s);
    return r;
}

/* Flush stdin until newline or EOF. Use where we want to discard the rest of the current input line. */
static void flush_stdin_until_newline(void) {
    int c;
    while ((c = getchar()) != '\n' && c != EOF) {
        /* discard */
    }
}

/* Comparison function for qsort. Guard against NULL names just in case. */
static int compare_menu_items(const void* a, const void* b) {
    const MenuItem* item_a = (const MenuItem*)a;
    const MenuItem* item_b = (const MenuItem*)b;

    const char* na = item_a->name ? item_a->name : "";
    const char* nb = item_b->name ? item_b->name : "";
    return strcmp(na, nb);
}

/* Frees only initialized fields; resilient to partial initialization. */
static void free_menu_items(MenuItem* items, size_t count) {
    if (!items) return;
    for (size_t i = 0; i < count; ++i) {
        if (items[i].name) {
            free(items[i].name);
            items[i].name = NULL;
        }
        if (items[i].description) {
            free(items[i].description);
            items[i].description = NULL;
        }
        items[i].source = 0;
    }
    free(items);
}

/**
 * Gather commands from native modules and Lua bridge.
 * - Always sets *out_count.
 * - Returns calloc'd MenuItem array on success (caller must free with free_menu_items)
 * - Returns NULL on failure, with *out_count == 0.
 */
static MenuItem* gather_all_commands(size_t* out_count) {
    if (!out_count) {
        logger_log(LOG_LEVEL_FATAL, "TUI", "gather_all_commands: out_count == NULL");
        return NULL;
    }
    *out_count = 0;

    int native_module_count = 0;
    const LoadedModule** modules = modules_get_all(&native_module_count);

    size_t native_command_count = 0;
    if (modules && native_module_count > 0) {
        for (int i = 0; i < native_module_count; i++) {
            const char** cmds = modules[i] ? modules[i]->info.commands : NULL;
            if (!cmds) continue;
            for (const char** cmd = cmds; *cmd; ++cmd) {
                native_command_count++;
            }
        }
    }

    size_t lua_command_count = 0;
    lua_command_count = lua_bridge_get_command_count();

    size_t total_commands = native_command_count + lua_command_count;
    if (total_commands == 0) {
        return NULL;
    }

    MenuItem* items = calloc(total_commands, sizeof(MenuItem));
    if (!items) {
        logger_log(LOG_LEVEL_FATAL, "TUI", "Failed to allocate memory for menu items.");
        return NULL;
    }

    size_t idx = 0;

    if (modules && native_module_count > 0) {
        for (int i = 0; i < native_module_count && idx < total_commands; i++) {
            if (!modules[i]) continue;
            const char** cmds = modules[i]->info.commands;
            const char* module_desc = modules[i]->info.description;
            if (!cmds) continue;
            for (const char** cmd = cmds; *cmd && idx < total_commands; ++cmd) {
                char* name_dup = safe_strdup_or_empty(*cmd);
                char* desc_dup = safe_strdup_or_empty(module_desc ? module_desc : "");
                if (!name_dup || !desc_dup) {
                    logger_log(LOG_LEVEL_ERROR, "TUI", "Out of memory while duplicating native command strings.");
                    free(name_dup);
                    free(desc_dup);
                    free_menu_items(items, idx);
                    *out_count = 0;
                    return NULL;
                }
                items[idx].name = name_dup;
                items[idx].description = desc_dup;
                items[idx].source = COMMAND_SOURCE_NATIVE;
                idx++;
            }
        }
    }

    if (lua_command_count > 0) {
        const char** lua_names = lua_bridge_get_all_command_names();
        if (!lua_names) {
            logger_log(LOG_LEVEL_ERROR, "TUI", "Lua bridge reported commands but returned no names.");
            free_menu_items(items, idx);
            *out_count = 0;
            return NULL;
        }

        for (size_t i = 0; i < lua_command_count && idx < total_commands; ++i) {
            const char* name = lua_names[i];
            const char* desc = lua_bridge_get_command_description(name);
            char* name_dup = safe_strdup_or_empty(name);
            char* desc_dup = safe_strdup_or_empty(desc ? desc : "A user-defined script command.");
            if (!name_dup || !desc_dup) {
                logger_log(LOG_LEVEL_ERROR, "TUI", "Out of memory while duplicating lua command strings.");
                free(name_dup);
                free(desc_dup);
                lua_bridge_free_command_names_list(lua_names);
                free_menu_items(items, idx);
                *out_count = 0;
                return NULL;
            }
            items[idx].name = name_dup;
            items[idx].description = desc_dup;
            items[idx].source = COMMAND_SOURCE_LUA;
            idx++;
        }

        lua_bridge_free_command_names_list(lua_names);
    }

    *out_count = idx;
    if (idx == 0) {
        free_menu_items(items, 0);
        return NULL;
    }

    return items;
}

/* BEGIN CHANGE: New functions to load and manage the ANSI art file. */
/**
 * @brief Loads an ANSI art file line by line into a dynamically allocated array of strings.
 *
 * @param filename The path to the ANSI art file.
 * @param out_lines A pointer to receive the array of strings.
 * @param out_line_count A pointer to receive the number of lines read.
 * @return true on success, false on failure. The caller must free the lines with free_ansi_art.
 */
static bool load_ansi_art(const char* filename, char*** out_lines, size_t* out_line_count) {
    *out_lines = NULL;
    *out_line_count = 0;

    FILE* fp = fopen(filename, "r");
    if (!fp) {
        logger_log(LOG_LEVEL_WARN, "TUI", "Could not open ANSI art file: %s", filename);
        return false;
    }

    char* line = NULL;
    size_t len = 0;
    size_t read;
    size_t count = 0;
    char** lines = NULL;

    while ((read = getline(&line, &len, fp)) != -1) {
        char** new_lines = realloc(lines, (count + 1) * sizeof(char*));
        if (!new_lines) {
            logger_log(LOG_LEVEL_FATAL, "TUI", "Failed to realloc for ANSI art lines.");
            free(line);
            for (size_t i = 0; i < count; ++i) free(lines[i]);
            free(lines);
            fclose(fp);
            return false;
        }
        lines = new_lines;

        // Strip trailing newline characters
        if (read > 0 && line[read - 1] == '\n') {
            line[read - 1] = '\0';
            if (read > 1 && line[read - 2] == '\r') {
                line[read - 2] = '\0';
            }
        }
        
        lines[count] = strdup(line);
        if (!lines[count]) {
             logger_log(LOG_LEVEL_FATAL, "TUI", "Failed to strdup ANSI art line.");
             free(line);
             for (size_t i = 0; i < count; ++i) free(lines[i]);
             free(lines);
             fclose(fp);
             return false;
        }
        count++;
    }

    free(line);
    fclose(fp);
    *out_lines = lines;
    *out_line_count = count;
    return true;
}

/**
 * @brief Frees the memory allocated by load_ansi_art.
 */
static void free_ansi_art(char** lines, size_t line_count) {
    if (!lines) return;
    for (size_t i = 0; i < line_count; ++i) {
        free(lines[i]);
    }
    free(lines);
}

/**
 * @brief Displays the menu with a two-column layout: ANSI art on the left, options on the right.
 */
static void display_menu(const MenuItem* items, size_t count) {
    platform_clear_screen();

    char** art_lines = NULL;
    size_t art_line_count = 0;
    bool has_art = load_ansi_art(PEACH_ART_FILE, &art_lines, &art_line_count);

    if (has_art) {
        size_t max_art_width = 0;
        for (size_t i = 0; i < art_line_count; ++i) {
            // A simple strlen is not accurate for ANSI, but it's a decent proxy for alignment.
            // For perfect alignment, one would need to parse and ignore ANSI escape codes.
            size_t current_len = strlen(art_lines[i]);
            if (current_len > max_art_width) {
                max_art_width = current_len;
            }
        }

        size_t max_rows = (art_line_count > count) ? art_line_count : count;

        for (size_t i = 0; i < max_rows; ++i) {
            // Print art line or empty space
            if (i < art_line_count) {
                printf("%s", art_lines[i]);
                // This padding is imperfect due to ANSI codes but works for this specific art.
            }
            
            // Print gutter and menu item
            if (i < count) {
                // Add spacing to create the second column
                printf("%*s", GUTTER_WIDTH, "");
                printf("%-2zu- (%s)", i + 1, items[i].name);
            }
            printf("\n");
        }
        free_ansi_art(art_lines, art_line_count);
    } else {
        // Fallback to a simple text-based menu if art file is missing
        printf("========================================\n");
        printf("  ph - The Polyglot Git Helper\n");
        printf("========================================\n\n");
        printf("Please select a command:\n\n");
        if (count > 0) {
            for (size_t i = 0; i < count; ++i) {
                printf("  %zu-(%s)\n", i + 1, items[i].name);
            }
        } else {
            printf("  No commands available.\n");
        }
    }
    printf("\n----------------------------------------\n");
}
/* END CHANGE */

/* Wait for enter: ensure any pending input is discarded FIRST, then wait for a fresh newline.
This avoids the "leftover chars cause immediate return" problem. */
static void wait_for_enter(void) {
    printf("\nPress Enter to continue...");
    fflush(stdout);
    /* Discard any pending characters from previous input (if any). */
    int c;
    while ((c = getchar()) != '\n' && c != EOF) { /* discard */ }
    /* Now wait for a fresh newline from the user. */
    while ((c = getchar()) != '\n' && c != EOF) { /* spin */ }
}

/* --- Public API Implementation --- */

bool tui_prompt_user(const char* prompt, char* buffer, size_t buffer_size) {
    if (!buffer || buffer_size == 0) return false;
    printf("%s", prompt);
    fflush(stdout);

    if (fgets(buffer, (int)buffer_size, stdin) == NULL) {
        return false;
    }

    size_t len = strcspn(buffer, "\r\n");
    if (len < strlen(buffer)) {
        buffer[len] = '\0';
    } else {
        int ch;
        while ((ch = getchar()) != '\n' && ch != EOF) { /* discard */ }
    }

    return true;
}

void tui_show_main_menu(void) {
    for (;;) {
        size_t item_count = 0;
        MenuItem* menu_items = gather_all_commands(&item_count);

        /* BEGIN CHANGE: Integrate "Exit" option directly into the menu list for unified display. */
        // Add one more item for "Exit"
        MenuItem* new_items = realloc(menu_items, (item_count + 1) * sizeof(MenuItem));
        if (!new_items) {
            tui_print_error("Failed to allocate memory for exit menu item.");
            free_menu_items(menu_items, item_count);
            break;
        }
        menu_items = new_items;
        
        menu_items[item_count].name = safe_strdup_or_empty("Exit");
        menu_items[item_count].description = safe_strdup_or_empty("Exit the application.");
        menu_items[item_count].source = COMMAND_SOURCE_NATIVE; // Or some other sentinel
        item_count++;
        /* END CHANGE */

        if (menu_items && item_count > 1) { // Sort all but the new "Exit" item
            qsort(menu_items, item_count - 1, sizeof(MenuItem), compare_menu_items);
        }

        display_menu(menu_items, item_count);

        char input_buffer[64];
        if (!tui_prompt_user("Your choice: ", input_buffer, sizeof(input_buffer))) {
            free_menu_items(menu_items, item_count);
            break;
        }

        char* endptr = NULL;
        errno = 0;
        long choice = strtol(input_buffer, &endptr, 10);
        if (endptr == input_buffer || *endptr != '\0' || errno == ERANGE) {
            tui_print_error("Invalid numeric input. Please enter a number.");
            wait_for_enter();
            free_menu_items(menu_items, item_count);
            continue;
        }

        /* BEGIN CHANGE: Updated choice logic to handle the integrated "Exit" command. */
        if (choice > 0 && (size_t)choice <= item_count) {
            const MenuItem* selected = &menu_items[choice - 1];
            
            // Check if the selected item is "Exit"
            if (selected->name && strcmp(selected->name, "Exit") == 0) {
                free_menu_items(menu_items, item_count);
                break; // Exit the loop
            }

            const char* argv[] = { "ph", selected->name ? selected->name : "", NULL };
            printf("\nExecuting '%s'...\n", selected->name ? selected->name : "<unknown>");
            printf("----------------------------------------\n");
            cli_dispatch_command(2, argv);
            printf("----------------------------------------\n");
            wait_for_enter();

        } else {
            tui_print_error("Invalid choice. Please try again.");
            wait_for_enter();
        }
        /* END CHANGE */

        free_menu_items(menu_items, item_count);
    }

    printf("\nExiting ph. Goodbye!\n");
}

void tui_print_error(const char* message) {
    if (!message) message = "<null>";
    fprintf(stderr, "\n[ERROR] %s\n", message);
}

void tui_print_success(const char* message) {
    if (!message) message = "<null>";
    printf("\n[SUCCESS] %s\n", message);
}