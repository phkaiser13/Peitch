# /* Copyright (C) 2025 Pedro Henrique / phkaiser13
# * File: Makefile
# * A user-friendly and convenient wrapper for the CMake build system.
# *
# * This Makefile does not contain any direct compilation logic. Instead, it serves
# * as a facade, providing simple, memorable commands that translate to the more
# * verbose commands required to operate CMake and CTest correctly. This greatly
# * improves the developer experience, especially on POSIX-like systems, by
# * standardizing common tasks like configuring, building, testing, and cleaning.
# *
# * SPDX-License-Identifier: Apache-2.0 */

# -----------------------------------------------------------------------------
# Variables
# -----------------------------------------------------------------------------

# Allow the user to override the CMake command (e.g., make CMAKE=cmake3)
CMAKE ?= cmake

# Build directory for all CMake-generated files and artifacts.
BUILD_DIR := build

# Name of the final executable produced by the build.
EXECUTABLE_NAME := ph

# Full path to the final executable for easy access.
EXECUTABLE_PATH := ${BUILD_DIR}/bin/${EXECUTABLE_NAME}

# Default CMake arguments. Can be extended for different build types.
CMAKE_ARGS := -S . -B ${BUILD_DIR}

# Pass-through arguments for the 'run' target.
# Example: make run ARGS="--version"
ARGS ?=

# -----------------------------------------------------------------------------
# Phony Targets (Targets that are not actual files)
# -----------------------------------------------------------------------------
# Ensures that 'make' will run the command even if a file with the same name exists.
.PHONY: all build configure configure-release test clean rebuild run install help

# -----------------------------------------------------------------------------
# Core Build Targets
# -----------------------------------------------------------------------------

# Default target: configure if needed, then build the project.
all: build

# Configure the project for a DEBUG build if it hasn't been configured yet.
# The existence of the CMake-generated Makefile is used as a sentinel file.
configure: ${BUILD_DIR}/Makefile

${BUILD_DIR}/Makefile: CMakeLists.txt
	@echo "--- Configuring project for DEBUG build ---"
	@${CMAKE} ${CMAKE_ARGS} -DCMAKE_BUILD_TYPE=Debug

# Configure the project for a RELEASE build. This is an explicit target.
configure-release:
	@echo "--- Configuring project for RELEASE build ---"
	@${CMAKE} ${CMAKE_ARGS} -DCMAKE_BUILD_TYPE=Release

# Build the project using the existing CMake configuration. Depends on configuration.
build: configure
	@echo "--- Building project ---"
	@${CMAKE} --build ${BUILD_DIR} --parallel

# Run all tests defined in the project using CTest.
# This target depends on the project being successfully built first.
test: build
	@echo "--- Running all tests ---"
	@cd ${BUILD_DIR} && ctest --output-on-failure

# -----------------------------------------------------------------------------
# Utility Targets
# -----------------------------------------------------------------------------

# Clean all build artifacts.
clean:
	@echo "--- Cleaning all build artifacts ---"
	@rm -rf ${BUILD_DIR}

# Clean and rebuild everything from scratch for a fresh start.
rebuild: clean all

# Build and run the main application, passing any specified ARGS.
run: build
	@echo "--- Running ph application ---"
	@${EXECUTABLE_PATH} ${ARGS}

# Install the application using the rules defined in CMakeLists.txt.
install: build
	@echo "--- Installing ph ---"
	@${CMAKE} --install ${BUILD_DIR}

# -----------------------------------------------------------------------------
# Help and Documentation
# -----------------------------------------------------------------------------

# Display a helpful message that explains all available targets.
help:
	@echo "ph Project Makefile Wrapper"
	@echo "---------------------------------"
	@echo "This Makefile provides a convenient interface for the CMake build system."
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Available Targets:"
	@echo "  all                 (Default) Configures (debug) and builds the project."
	@echo "  build               Builds the project using the existing configuration."
	@echo "  configure           Configures the project for a DEBUG build."
	@echo "  configure-release   Configures the project for a RELEASE build."
	@echo "  rebuild             Cleans all build files and rebuilds the project."
	@echo "  test                Builds the project and runs all automated tests via CTest."
	@echo "  run                 Builds and runs the main application. Use ARGS=\"...\" to pass arguments."
	@echo "                      Example: make run ARGS=\"--version\""
	@echo "  install             Installs the application to the configured directory."
	@echo "  clean               Removes all build artifacts and directories."
	@echo "  help                Displays this help message."
