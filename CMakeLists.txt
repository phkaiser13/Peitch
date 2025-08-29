# /* Copyright (C) 2025 Pedro Henrique / phkaiser13
# * CMakeLists.txt - Main build script for the ph project.
# *
# * This file orchestrates the entire build process, including:
# * 1. Finding and configuring all C/C++ dependencies (Lua, libcurl).
# * 2. Compiling the main C/C++ core application (`ph`).
# * 3. Compiling all polyglot modules (Rust and C++) into shared libraries.
# * 4. Placing all build artifacts into a clean `bin` directory structure.
# * 5. Defining installation rules.
# *
# * It has been updated to handle the full migration to Rust modules, replacing
# * all previous Go build commands with robust Rust/Cargo integration.
# *
# * Important: Some comments still in portuguese.
# * SPDX-License-Identifier: Apache-2.0 */

cmake_minimum_required(VERSION 3.15)

project(ph LANGUAGES C CXX VERSION 1.0.0)

# --- Global Settings and Options ---
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
include(FetchContent)

# Define output directories for a clean build structure
set(EXECUTABLE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/bin)
set(LIBRARY_OUTPUT_PATH ${CMAKE_BINARY_DIR}/bin)
set(MODULE_OUTPUT_PATH ${CMAKE_BINARY_DIR}/bin/modules)
file(MAKE_DIRECTORY ${MODULE_OUTPUT_PATH})


# ==========================================================================================
# BEGIN: MERGED DEPENDENCY DISCOVERY LOGIC
# This section is preserved as-is from the original file.
# ==========================================================================================

# Allow users / CI to pass overrides
if(DEFINED ENV{LUA_DIR} AND NOT DEFINED LUA_DIR)
set(LUA_DIR $ENV{LUA_DIR})
endif()
if(DEFINED ENV{CURL_DIR} AND NOT DEFINED CURL_DIR)
set(CURL_DIR $ENV{CURL_DIR})
endif()
if(DEFINED ENV{VCPKG_ROOT} AND NOT DEFINED CMAKE_TOOLCHAIN_FILE)
set(CMAKE_TOOLCHAIN_FILE "$ENV{VCPKG_ROOT}/scripts/buildsystems/vcpkg.cmake" CACHE STRING "vcpkg toolchain")
endif()

# Add common Homebrew prefixes on macOS
if(APPLE)
if(EXISTS "/opt/homebrew/opt")
list(APPEND CMAKE_PREFIX_PATH "/opt/homebrew/opt")
endif()
if(EXISTS "/usr/local/opt")
list(APPEND CMAKE_PREFIX_PATH "/usr/local/opt")
endif()
endif()

# --- Helper: try pkg-config with candidate names ---
find_package(PkgConfig QUIET)
function(try_pkg_config OUT_PREFIX)
set(options)
set(oneValueArgs)
set(multiValueArgs NAMES)
cmake_parse_arguments(PKGC "${options}" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})

if(NOT PKG_CONFIG_FOUND)
return()
endif()

foreach(name IN LISTS PKGC_NAMES)
pkg_check_modules(_pc_${OUT_PREFIX} ${name} QUIET)
if(_pc_${OUT_PREFIX}_FOUND)
set(${OUT_PREFIX}_PKG_NAME ${name} PARENT_SCOPE)
string(TOUPPER ${OUT_PREFIX} _UP)
set(${_UP}_PKG_FOUND TRUE PARENT_SCOPE)
set(${_UP}_PKG_INCLUDE_DIRS ${_pc_${OUT_PREFIX}_INCLUDE_DIRS} PARENT_SCOPE)
set(${_UP}_PKG_LIBRARY_DIRS ${_pc_${OUT_PREFIX}_LIBRARY_DIRS} PARENT_SCOPE)
set(${_UP}_PKG_LIBRARIES ${_pc_${OUT_PREFIX}_LIBRARIES} PARENT_SCOPE)
return()
endif()
endforeach()
endfunction()

# --- Find CURL (prefer find_package, fallback to pkg-config) ---
set(CURL_TARGET_NAME "")
find_package(CURL QUIET)
if(CURL_FOUND)
if(TARGET CURL::libcurl)
set(CURL_TARGET_NAME "CURL::libcurl")
else()
add_library(CURL::libcurl UNKNOWN IMPORTED)
set_target_properties(CURL::libcurl PROPERTIES
IMPORTED_LOCATION "${CURL_LIBRARY}"
INTERFACE_INCLUDE_DIRECTORIES "${CURL_INCLUDE_DIRS}"
)
set(CURL_TARGET_NAME "CURL::libcurl")
endif()
else()
try_pkg_config(CURL libcurl curl)
if(CURL_PKG_FOUND)
add_library(CURL::libcurl UNKNOWN IMPORTED)
set_target_properties(CURL::libcurl PROPERTIES
INTERFACE_INCLUDE_DIRECTORIES "${CURL_PKG_INCLUDE_DIRS}"
INTERFACE_LINK_LIBRARIES "${CURL_PKG_LIBRARIES}"
)
set(CURL_TARGET_NAME "CURL::libcurl")
endif()
endif()

if(NOT CURL_TARGET_NAME)
message(FATAL_ERROR "Could not locate libcurl. Use system package manager or provide CURL_DIR.")
endif()

# --- Find Lua (try find_package, then pkg-config with multiple names) ---
set(LUA_TARGET_NAME "")
find_package(Lua 5.4 QUIET)
if(Lua_FOUND)
if(TARGET Lua::Lua)
set(LUA_TARGET_NAME "Lua::Lua")
else()
if(DEFINED LUA_LIBRARIES)
set(_lua_lib "${LUA_LIBRARIES}")
elseif(DEFINED LUA_LIBRARY)
set(_lua_lib "${LUA_LIBRARY}")
endif()
if(DEFINED LUA_INCLUDE_DIR)
set(_lua_inc "${LUA_INCLUDE_DIR}")
elseif(DEFINED LUA_INCLUDE_DIRS)
set(_lua_inc "${LUA_INCLUDE_DIRS}")
endif()
add_library(Lua::Lua INTERFACE IMPORTED)
if(_lua_lib)
set_target_properties(Lua::Lua PROPERTIES INTERFACE_LINK_LIBRARIES "${_lua_lib}")
endif()
if(_lua_inc)
set_target_properties(Lua::Lua PROPERTIES INTERFACE_INCLUDE_DIRECTORIES "${_lua_inc}")
endif()
set(LUA_TARGET_NAME "Lua::Lua")
endif()
else()
try_pkg_config(LUA lua5.4 lua-5.4 lua liblua)
if(LUA_PKG_FOUND)
add_library(Lua::Lua UNKNOWN IMPORTED)
set_target_properties(Lua::Lua PROPERTIES
INTERFACE_INCLUDE_DIRECTORIES "${LUA_PKG_INCLUDE_DIRS}"
INTERFACE_LINK_LIBRARIES "${LUA_PKG_LIBRARIES}"
)
set(LUA_TARGET_NAME "Lua::Lua")
endif()
endif()

if(NOT LUA_TARGET_NAME)
message(FATAL_ERROR "Could not locate Lua 5.4 development files. Install it or provide LUA_DIR.")
endif()

# --- Find nlohmann_json (try package, else FetchContent) ---
find_package(nlohmann_json 3.2.0 QUIET)
if(NOT nlohmann_json_FOUND)
message(STATUS "nlohmann_json not found on system. Fetching via FetchContent...")
FetchContent_Declare(
nlohmann_json
GIT_REPOSITORY https://github.com/nlohmann/json.git
GIT_TAG v3.11.3
)
FetchContent_MakeAvailable(nlohmann_json)
endif()

# ==========================================================================================
# END: MERGED DEPENDENCY DISCOVERY LOGIC
# ==========================================================================================


# --- Compile C/C++ Core ---

# Logger library (static)
add_library(logger STATIC src/libs/liblogger/Logger.cpp)
target_include_directories(logger PUBLIC ${CMAKE_SOURCE_DIR}/src/ipc/include)

# Main executable
# NOTE: The GLOB_RECURSE already finds all .c files, including the new handlers.
# No manual additions are needed here, which is more maintainable.
file(GLOB_RECURSE CORE_SOURCES
"src/core/cli/*.c"
"src/core/config/*.c"
"src/core/main/*.c"
"src/core/module_loader/*.c"
"src/core/platform/*.c"
"src/core/scripting/*.c"
"src/core/tui/*.c"
"src/libs/libcommon/*.c"
)
add_executable(ph ${CORE_SOURCES})

target_include_directories(ph PUBLIC
${CMAKE_SOURCE_DIR}/src
${CMAKE_SOURCE_DIR}/src/core
${CMAKE_SOURCE_DIR}/src/ipc/include
${CMAKE_SOURCE_DIR}/src/libs
${LUA_INCLUDE_DIRS}
${LUA_PKG_INCLUDE_DIRS}
${CURL_INCLUDE_DIRS}
${CURL_PKG_INCLUDE_DIRS}
)

# =============================================================================
# START: NEW SCALABLE RUST BUILD SYSTEM
# =============================================================================

# --- Step 2: Create a Function to Compile Rust Modules ---
# This function encapsulates the logic for building a Rust FFI module in release
# mode and creating a custom target for it.
function(build_rust_module module_name)
    get_filename_component(module_real_name ${module_name} NAME)
    set(RUST_MODULE_DIR ${CMAKE_SOURCE_DIR}/src/modules/${module_name})
    set(RUST_TARGET_DIR ${CMAKE_BINARY_DIR}/modules_build/${module_real_name})
    
    # Determine the correct library name based on the OS
    if(WIN32)
        set(RUST_LIB_FILENAME "${module_real_name}.dll")
    elseif(APPLE)
        set(RUST_LIB_FILENAME "lib${module_real_name}.dylib")
    else()
        set(RUST_LIB_FILENAME "lib${module_real_name}.so")
    endif()

    set(RUST_LIB_OUTPUT_PATH ${RUST_TARGET_DIR}/target/release/${RUST_LIB_FILENAME})

    add_custom_command(
        OUTPUT ${RUST_LIB_OUTPUT_PATH}
        COMMAND ${CMAKE_COMMAND} -E make_directory ${RUST_TARGET_DIR}
        COMMAND cargo build --release --manifest-path ${RUST_MODULE_DIR}/Cargo.toml --target-dir ${RUST_TARGET_DIR}/target
        WORKING_DIRECTORY ${RUST_MODULE_DIR}
        COMMENT "Building Rust module: ${module_name}"
        VERBATIM
    )

    add_custom_target(
        build_${module_real_name} ALL
        DEPENDS ${RUST_LIB_OUTPUT_PATH}
    )
endfunction()

# --- Step 3: Define and Build All Required Rust FFI Modules ---
# This list defines all modern Rust modules that are part of the application.
set(RUST_MODULES
    docker_ops
    domains/kubernetes/k8s_ops
    cloud_ops
    git_sync
    domains/kubernetes/k8s_health
    domains/kubernetes/k8s_info
    domains/kubernetes/k8s_local_dev
    domains/kubernetes/k8s_preview
    domains/kubernetes/k8s_sync_manager
    multi_cluster_orchestrator
    policy_engine
    release_orchestrator
    runner_manager
    secret_manager
    rbac_manager
    signature_verifier
    tracing_layer
)

foreach(module IN LISTS RUST_MODULES)
    build_rust_module(${module})
endforeach()

# =============================================================================
# END: NEW SCALABLE RUST BUILD SYSTEM
# =============================================================================

# --- Link core executable against its dependencies ---
target_link_libraries(ph
    PRIVATE
    logger
    ${CURL_TARGET_NAME}
    ${LUA_TARGET_NAME}
)
if(UNIX AND NOT APPLE)
    target_link_libraries(ph PRIVATE dl m)
endif()

# --- Step 4: Link the Executable Principal with the Modules Rust ---
# This loop programmatically adds each compiled Rust library to the linker command.
# This is more maintainable than a hardcoded list of paths.
foreach(module IN LISTS RUST_MODULES)
    get_filename_component(module_real_name ${module} NAME)
    if(WIN32)
        set(RUST_LIB_FILENAME "${module_real_name}.dll")
    elseif(APPLE)
        set(RUST_LIB_FILENAME "lib${module_real_name}.dylib")
    else()
        set(RUST_LIB_FILENAME "lib${module_real_name}.so")
    endif()
    
    target_link_libraries(ph PRIVATE ${CMAKE_BINARY_DIR}/modules_build/${module_real_name}/target/release/${RUST_LIB_FILENAME})
endforeach()


# --- Compile Polyglot Modules (LEGACY) ---
# This section is preserved for any older modules that still use it.
# The new `build_rust_module` function is the preferred method going forward.
macro(add_rust_module module_path library_name)
    set(OUTPUT_LIB_PATH ${MODULE_OUTPUT_PATH}/${CMAKE_SHARED_LIBRARY_PREFIX}${library_name}${CMAKE_SHARED_LIBRARY_SUFFIX})
    set(SOURCE_LIB_PATH ${CMAKE_BINARY_DIR}/cargo_target/debug/${CMAKE_SHARED_LIBRARY_PREFIX}${library_name}${CMAKE_SHARED_LIBRARY_SUFFIX})
    add_custom_command(
            OUTPUT ${OUTPUT_LIB_PATH}
            COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${CMAKE_BINARY_DIR}/cargo_target" cargo build --manifest-path ${CMAKE_SOURCE_DIR}/src/modules/${module_path}/Cargo.toml
            COMMAND ${CMAKE_COMMAND} -E copy_if_different ${SOURCE_LIB_PATH} ${MODULE_OUTPUT_PATH}/
            WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/src/modules/${module_path}
            COMMENT "Compiling Rust module in DEBUG mode: ${library_name}"
            VERBATIM
    )
    add_custom_target(build_${library_name} ALL
            DEPENDS ${OUTPUT_LIB_PATH}
    )
endmacro()

# --- Build All Modules (LEGACY) ---
add_rust_module(git_ops git_ops)
add_rust_module(issue_tracker issue_tracker)
add_rust_module(sync_engine sync_engine)
add_rust_module(api_client api_client)
add_rust_module(devops_automation devops_automation)
add_rust_module(ci_cd_manager/parser workflow_parser)

# Build the C++ visualizer module
add_library(ci_cd_visualizer SHARED src/modules/ci_cd_manager/visualizer/PipelineVisualizer.cpp)
target_include_directories(ci_cd_visualizer PUBLIC
${CMAKE_SOURCE_DIR}/src/ipc/include
)
target_link_libraries(ci_cd_visualizer PRIVATE
logger
nlohmann_json::nlohmann_json
)
set_target_properties(ci_cd_visualizer PROPERTIES
LIBRARY_OUTPUT_DIRECTORY "${MODULE_OUTPUT_PATH}"
)

# --- Installation Rules ---
install(TARGETS ph DESTINATION bin COMPONENT Runtime)

# --- Step 5: Install the Rust FFI modules alongside the executable ---
# This loop ensures all compiled Rust libraries are copied to the installation
# directory, making the application portable.
foreach(module IN LISTS RUST_MODULES)
    get_filename_component(module_real_name ${module} NAME)
    if(WIN32)
        set(RUST_LIB_FILENAME "${module_real_name}.dll")
    elseif(APPLE)
        set(RUST_LIB_FILENAME "lib${module_real_name}.dylib")
    else()
        set(RUST_LIB_FILENAME "lib${module_real_name}.so")
    endif()

    install(
        FILES ${CMAKE_BINARY_DIR}/modules_build/${module_real_name}/target/release/${RUST_LIB_FILENAME}
        DESTINATION bin # Install alongside the 'ph' executable
        COMPONENT Runtime
    )
endforeach()

# --- Preserved Sections (Installer Engine, Legacy Install Rules) ---
# The following sections are preserved from the original file without modification.
# if(ph_BUILD_INSTALLER)
#    message(STATUS "Installer engine build is enabled. Adding subdirectory.")
#   add_subdirectory(installer)
# else()
#    message(STATUS "Installer engine build is disabled.")
# endif()

# install(
#    DIRECTORY ${MODULE_OUTPUT_PATH}/
#    DESTINATION bin/modules
#    FILES_MATCHING PATTERN "*.so" PATTERN "*.dll" PATTERN "*.dylib"
# )

# if(ph_BUILD_INSTALLER)
#     install(TARGETS installer DESTINATION bin)
#     install(
#         FILES ${CMAKE_SOURCE_DIR}/installer/config.json
#         DESTINATION bin
#     )
# endif()
