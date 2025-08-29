# =================================================================================================
# Stage 1: The Builder Environment
#
# In this stage, we will install all the necessary tools and dependencies to compile the entire
# application from source. This includes the C/C++ compiler, CMake, git, and any other libraries
# that the project requires. The resulting image from this stage will be large and is only used
# for building the artifact. It will be discarded later to keep the final image small.
# =================================================================================================

# We use a specific version of Ubuntu as a base. Using a specific tag instead of 'latest'
# ensures that our build is reproducible.
ARG UBUNTU_VERSION=22.04
FROM ubuntu:${UBUNTU_VERSION} AS builder

# Set a label to identify this stage. Useful for debugging.
LABEL stage="builder"

# Set the DEBIAN_FRONTEND to noninteractive to prevent the package manager from
# prompting for user input during automated builds.
ENV DEBIAN_FRONTEND=noninteractive

# Install Build Dependencies
# We update the package list and then install all required tools in a single RUN command.
# This helps to reduce the number of layers in our Docker image.
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Core build tools for C/C++
    build-essential \
    g++ \
    gcc \
    make \
    # CMake is the build system used by the project
    cmake \
    # Git is needed to clone repositories or fetch version information
    git \
    # Common libraries that are often required
    pkg-config \
    # NOTE: Add any other project-specific development libraries here.
    # For example, if your project needs libcurl or openssl for networking:
    # libcurl4-openssl-dev \
    # libssl-dev \
    # Based on the project structure, Lua seems to be used.
    liblua5.4-dev \
    # Clean up the apt cache to reduce layer size
    && rm -rf /var/lib/apt/lists/*

# Set the working directory inside the container. All subsequent commands (COPY, RUN)
# will be executed from this path.
WORKDIR /app

# Copy the entire project source code into the builder stage.
# The '.' in the source path refers to the build context (the directory where you run `docker build`).
# The '.' in the destination path refers to the current WORKDIR (/app).
COPY . .

# Compile the Application
# We will create a separate build directory to keep the build artifacts
# isolated from the source code. This is a standard CMake practice.
RUN cmake -B build -S . && \
    cmake --build build --parallel

# At this point, inside the `/app/build/` directory, we should have our compiled
# executable. Let's assume its name is 'peitch'.


# =================================================================================================
# Stage 2: The Final Runtime Image
#
# This is the final stage. We will build the actual image that will be distributed and run.
# It starts from a minimal base image to reduce size and potential security vulnerabilities.
# We will copy ONLY the compiled binary from the 'builder' stage into this final image.
# =================================================================================================

# We use a slim Debian version for a smaller footprint.
# Using a specific tag ensures reproducibility.
ARG DEBIAN_VERSION=bullseye-slim
FROM debian:${DEBIAN_VERSION}

# Metadata labels for the image, following OCI (Open Container Initiative) standards.
# It's good practice to document who maintains the image and where the source is.
LABEL maintainer="phdev13 <pedro.garcia@vytruve.org>"
LABEL org.opencontainers.image.title="Peitch CLI Tool"
LABEL org.opencontainers.image.description="A Docker image test for the Peitch DevOps CLI tool."
LABEL org.opencontainers.image.source="https://github.com/phkaiser13/peitch"

# Update the package list and install only the essential runtime dependencies.
# For a C/C++ application, these are often just the standard C libraries, which are
# usually already present in the base image.
# NOTE: If `ldd` on your binary (in the builder stage) shows other `.so` dependencies,
# install the corresponding packages here.
RUN apt-get update && apt-get install -y --no-install-recommends \
    # For example, if the app needs libcurl at runtime:
    # libcurl4 \
    # The Lua runtime library is likely needed.
    liblua5.4-0 \
    # Clean up the apt cache to reduce the final image size.
    && rm -rf /var/lib/apt/lists/*

# Security Best Practice: Create a dedicated, non-root user to run the application.
# Running containers as a non-root user is a critical security measure to limit the
# blast radius in case of a container compromise.
ARG APP_USER=peitch
ARG APP_UID=1001
ARG APP_GID=1001
RUN groupadd -g ${APP_GID} ${APP_USER} && \
    useradd -u ${APP_UID} -g ${APP_GID} -s /bin/bash -m ${APP_USER}

# Copy the compiled binary from the 'builder' stage.
# The `--from=builder` flag tells Docker to copy from the stage named 'builder'.
# We are copying the executable to a standard location in the PATH.
# IMPORTANT: You might need to change 'peitch' to the actual name of your main executable.
COPY --from=builder /app/build/peitch /usr/local/bin/peitch

# Switch to the non-root user. Any subsequent commands (ENTRYPOINT, CMD)
# will be executed as this user.
USER ${APP_USER}

# Set the working directory for the non-root user.
WORKDIR /home/${APP_USER}

# Set the default command to be executed when the container starts.
# 'ENTRYPOINT' is used for the main executable of the container.
ENTRYPOINT ["peitch"]

# 'CMD' provides default arguments to the 'ENTRYPOINT'.
# In this case, if the container is run without any arguments, it will execute 'peitch --help'.
# This can be easily overridden from the `docker run` command line.
CMD ["--help"]