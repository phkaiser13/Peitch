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
    /* lua_bridge_get_command_count may return 0 on error or no commands; assume it returns valid size_t */
    lua_command_count = lua_bridge_get_command_count();

    size_t total_commands = native_command_count + lua_command_count;
    if (total_commands == 0) {
        // no commands found -> return NULL but set out_count to 0 as contract
        return NULL;
    }

    /* Allocate zeroed items to allow safe cleanup on partial failure. */
    MenuItem* items = calloc(total_commands, sizeof(MenuItem));
    if (!items) {
        logger_log(LOG_LEVEL_FATAL, "TUI", "Failed to allocate memory for menu items.");
        return NULL;
    }

    size_t idx = 0;

    /* Populate native commands (if any) */
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
                    /* cleanup of already created entries */
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

    /* Populate Lua commands (if any) */
    if (lua_command_count > 0) {
        const char** lua_names = lua_bridge_get_all_command_names();
        if (!lua_names) {
            /* Unexpected: count > 0 but names list NULL -> treat as error */
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

    /* Sanity check: idx should equal total_commands, but allow if fewer (unexpectedly) */
    *out_count = idx;
    if (idx == 0) {
        free_menu_items(items, 0);
        return NULL;
    }

    return items;
}

/* Compute column width for nicer alignment; cap to avoid super-wide layouts. */
static size_t compute_name_column_width(const MenuItem* items, size_t count) {
    size_t max = 0;
    const size_t cap = 40; /* max column width */
    for (size_t i = 0; i < count; ++i) {
        if (!items[i].name) continue;
        size_t len = strlen(items[i].name);
        if (len > max) max = len;
    }
    if (max > cap) max = cap;
    if (max < 8) max = 8; /* minimum width for aesthetics */
    return max;
}

static void display_menu(const MenuItem* items, size_t count) {
    platform_clear_screen();
    printf("========================================\n");
    printf("  ph - The Polyglot Git Helper\n");
    printf("========================================\n\n");
    printf("Please select a command:\n\n");

    if (count > 0 && items) {
        size_t name_col = compute_name_column_width(items, count);
        for (size_t i = 0; i < count; ++i) {
            const char* name = items[i].name ? items[i].name : "";
            const char* desc = items[i].description ? items[i].description : "";
            /* If name is longer than column width, print truncated with ellipsis (safe) */
            if (strlen(name) > name_col) {
                char truncated[64];
                /* calculate how many chars we can take so that + "..." fits */
                size_t avail = sizeof(truncated) - 1; /* room for NUL */
                size_t dot_len = 3; /* "..." */
                size_t take = name_col > dot_len ? (name_col - dot_len) : 1;
                if (take > avail - dot_len) take = avail - dot_len;
                /* use snprintf for safe bounded copy + ellipsis */
                int written = snprintf(truncated, sizeof(truncated), "%.*s...", (int)take, name);
                if (written < 0 || (size_t)written >= sizeof(truncated)) {
                    /* fallback in improbable snprintf failure */
                    strncpy(truncated, name, sizeof(truncated) - 4);
                    truncated[sizeof(truncated) - 4] = '\0';
                    strcat(truncated, "...");
                }
                printf("  [%2zu] %-*s - %s\n", i + 1, (int)name_col, truncated, desc);
            } else {
                printf("  [%2zu] %-*s - %s\n", i + 1, (int)name_col, name, desc);
            }
        }
    } else {
        printf("  No commands available.\n");
    }

    printf("\n  [%2zu] Exit\n", count + 1);
    printf("\n----------------------------------------\n");
}

/* Wait for enter: ensure any pending input is discarded FIRST, then wait for a fresh newline.
   This avoids the "leftover chars cause immediate return" problem. */
static void wait_for_enter(void) {
    /* Discard any pending characters from previous input (if any). */
    flush_stdin_until_newline();

    printf("\nPress Enter to continue...");
    /* Now wait for a fresh newline from the user. */
    int c;
    while ((c = getchar()) != '\n' && c != EOF) {
        /* spin until newline (user pressed Enter) or EOF */
    }
}

/* --- Public API Implementation --- */

/*
 * Enhanced prompt: reads a line into buffer (size buffer_size).
 * If the line is longer than buffer_size-1, the remainder of that input line is discarded
 * so we don't leave junk in stdin for subsequent reads.
 */
bool tui_prompt_user(const char* prompt, char* buffer, size_t buffer_size) {
    if (!buffer || buffer_size == 0) return false;
    printf("%s", prompt);
    fflush(stdout);

    if (fgets(buffer, (int)buffer_size, stdin) == NULL) {
        return false;
    }

    size_t len = strcspn(buffer, "\r\n");
    if (len < strlen(buffer)) {
        /* we found a newline inside buffer; strip it */
        buffer[len] = '\0';
    } else {
        /* No newline inside the buffer => the input line was longer than buffer - 1
           => discard the remainder up to newline so the next read starts fresh. */
        int ch;
        while ((ch = getchar()) != '\n' && ch != EOF) { /* discard */ }
    }

    return true;
}

void tui_show_main_menu(void) {
    for (;;) {
        size_t item_count = 0;
        MenuItem* menu_items = gather_all_commands(&item_count);

        if (menu_items && item_count > 0) {
            qsort(menu_items, item_count, sizeof(MenuItem), compare_menu_items);
        }

        display_menu(menu_items, item_count);

        /* Prompt the user */
        char input_buffer[64];
        if (!tui_prompt_user("Your choice: ", input_buffer, sizeof(input_buffer))) {
            /* EOF or error */
            free_menu_items(menu_items, item_count);
            break;
        }

        /* Robust parsing of integer input */
        char* endptr = NULL;
        errno = 0;
        long choice = strtol(input_buffer, &endptr, 10);
        if (endptr == input_buffer || *endptr != '\0' || errno == ERANGE) {
            tui_print_error("Invalid numeric input. Please enter a number.");
            wait_for_enter();
            free_menu_items(menu_items, item_count);
            continue;
        }

        if (choice > 0 && (size_t)choice <= item_count) {
            /* safe because choice is within item_count */
            const MenuItem* selected = &menu_items[choice - 1];
            const char* argv[] = { "ph", selected->name ? selected->name : "", NULL };

            printf("\nExecuting '%s'...\n", selected->name ? selected->name : "<unknown>");
            printf("----------------------------------------\n");
            cli_dispatch_command(2, argv);
            printf("----------------------------------------\n");
            wait_for_enter();

        } else if (choice == (long)item_count + 1) {
            free_menu_items(menu_items, item_count);
            break; /* Exit */
        } else {
            tui_print_error("Invalid choice. Please try again.");
            wait_for_enter();
        }

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
