# tests/CMakeLists.txt

# --- Unit Test for the C Core ---

# Add the executable for our test runner.
# It includes the C source file we want to test and the test implementation.
add_executable(core_unit_tests
    ../src/core/config/config_manager.c
    test_config_manager.c
)

# Link the test executable against the logger and common utilities,
# just like the main application, so it can use those functions.
target_link_libraries(core_unit_tests PRIVATE logger)

# Add the necessary include directories so the test code can find the headers.
target_include_directories(core_unit_tests PUBLIC
    ../src
    ../src/core
    ../src/ipc/include
    ../src/libs
)

# --- CTest Integration ---

# Enable testing for this directory.
enable_testing()

# Add the compiled test executable as a CTest test.
# The first argument is the name of the test, the second is the command to run.
add_test(NAME CoreConfigManagerTest COMMAND core_unit_tests)
