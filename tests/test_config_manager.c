// tests/test_config_manager.c
// Simple test runner for the configuration manager.

#include "config/config_manager.h"
#include <stdio.h>
#include <string.h>
#include "libs/liblogger/logger.hpp" // <-- FIX: Include the logger header
#include <assert.h>

// Helper to create a temporary config file for testing.
void create_test_config_file(const char* filename) {
    FILE* f = fopen(filename, "w");
    assert(f != NULL);
    fprintf(f, "# This is a comment\n");
    fprintf(f, "  key1 = value1  \n");
    fprintf(f, "key2=value2\n");
    fprintf(f, "\n"); // Empty line
    fprintf(f, "malformed_line\n");
    fprintf(f, "key3 = with spaces\n");
    fclose(f);
}

void test_config_loading_and_retrieval() {
    printf("Running test: test_config_loading_and_retrieval...\n");

    const char* test_filename = "test_config.conf";
    create_test_config_file(test_filename);

    // Load the config file.
    // In a real scenario, you might want to redirect logger output.
    assert(config_load(test_filename) == ph_SUCCESS);

    // Test successful key-value lookups
    const char* val1 = config_get_value("key1");
    assert(val1 != NULL && strcmp(val1, "value1") == 0);
    printf("  [PASS] key1 => value1\n");

    const char* val2 = config_get_value("key2");
    assert(val2 != NULL && strcmp(val2, "value2") == 0);
    printf("  [PASS] key2 => value2\n");

    const char* val3 = config_get_value("key3");
    assert(val3 != NULL && strcmp(val3, "with spaces") == 0);
    printf("  [PASS] key3 => 'with spaces'\n");

    // Test lookup of a non-existent key
    const char* val_nonexistent = config_get_value("nonexistent");
    assert(val_nonexistent == NULL);
    printf("  [PASS] Non-existent key returns NULL\n");

    // Cleanup
    config_cleanup();
    remove(test_filename);
    printf("Test finished.\n\n");
}

int main() {
    // Initialize necessary subsystems, like the logger, if tests depend on them.
    logger_init("test_log.txt");

    test_config_loading_and_retrieval();

    logger_cleanup();
    printf("All C core tests passed!\n");
    return 0;
}
