# /* Copyright (C) 2025 Pedro Henrique / phkaiser13
# * Makefile
# * This Makefile is responsible for building the 'phpkg' module.
# * It compiles both C and C++ source files into object files and then links
# * them together to create a single shared library (e.g., phpkg.so). This
# * allows the module to be loaded dynamically by the main application core.
# * The build process is optimized for performance and includes standard
# * warnings to ensure code quality.
# * SPDX-License-Identifier: Apache-2.0 */

# Compiler definitions
CC = gcc
CXX = g++

# Target shared library
TARGET = phpkg.so

# Compiler flags
# -O2: Optimization level 2
# -Wall -Wextra: Enable all major warnings for better code quality
# -fPIC: Generate Position-Independent Code, necessary for shared libraries
# -I.: Include the current directory for local headers
CFLAGS = -O2 -Wall -Wextra -fPIC -I.
CXXFLAGS = -O2 -Wall -Wextra -fPIC -I. -std=c++11

# Linker flags
# -shared: Create a shared library
# -lstdc++: Link against the C++ standard library, required by downloader.cpp
LDFLAGS = -shared -lstdc++

# Source files
C_SRCS = phpkg.c packages.c installer.c
CXX_SRCS = downloader.cpp

# Object files (derived from source files)
C_OBJS = $(C_SRCS:.c=.o)
CXX_OBJS = $(CXX_SRCS:.cpp=.o)
OBJS = $(C_OBJS) $(CXX_OBJS)

# Default target: build the shared library
all: $(TARGET)

# Rule to link the shared library
# We use CXX for linking because it correctly handles linking C++ object files
# and the necessary standard libraries.
$(TARGET): $(OBJS)
	@echo "==> Linking shared library: $@"
	$(CXX) $(LDFLAGS) -o $@ $(OBJS)
	@echo "==> Build complete: $(TARGET)"

# Rule to compile C source files into object files
%.o: %.c
	@echo "==> Compiling C source: $<"
	$(CC) $(CFLAGS) -c $< -o $@

# Rule to compile C++ source files into object files
%.o: %.cpp
	@echo "==> Compiling C++ source: $<"
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Clean up build artifacts
clean:
	@echo "==> Cleaning up build artifacts..."
	rm -f $(OBJS) $(TARGET)
	@echo "==> Cleanup complete."

# Phony targets are not real files
.PHONY: all clean