#/*
# * Copyright (C) 2025 Pedro Henrique / phkaiser13
# *
# * File: production-safeguards.rego
# *
# * This Rego policy file establishes a comprehensive set of production safeguards for Kubernetes workloads,
# * enforced via the Open Policy Agent (OPA). The primary goal of this file is to prevent common misconfigurations
# * that could lead to security vulnerabilities, instability, or inefficient resource utilization in a production
# * environment.
# *
# * Architecture:
# * The policies are structured as a series of `deny` rules within the `k8s.policies.production_safeguards` package.
# * Each rule checks for a specific undesirable configuration in the input Kubernetes manifest. If a violation
# * is found, the rule evaluates to true and generates a detailed, user-friendly error message. This approach
# * allows for clear, actionable feedback during CI/CD checks or admission control.
# *
# * Key Policies Implemented:
# * 1.  Disallow 'latest' tag: Prevents the use of ambiguous and mutable image tags.
# * 2.  Require Resource Limits: Enforces the presence of CPU and memory limits for all containers.
# * 3.  Prohibit Privileged Containers: Blocks containers that request elevated privileges.
# * 4.  Disallow Host Network/PID: Prevents containers from sharing the host's network or process ID namespace.
# * 5.  Prevent Running as Root: Ensures containers do not run with root user privileges.
# * 6.  Enforce Read-Only Root Filesystem: Mandates that container root filesystems are immutable.
# * 7.  Require Liveness and Readiness Probes: Ensures deployments are properly monitored for health and availability.
# *
# * Role in the System:
# * This file acts as a critical gatekeeper in the CI/CD pipeline. By integrating it with tools like Conftest or
# * Gatekeeper, it automatically validates Kubernetes manifests against these production-readiness standards
# * before they can be deployed, thus upholding engineering excellence and system robustness.
# *
# * SPDX-License-Identifier: Apache-2.0
# */

package k8s.policies.production_safeguards

#// Fetches all containers from a given Kubernetes resource manifest.
#// This is a helper function to simplify policy rules by abstracting the process of
#// finding container specifications within different kinds of workload resources (e.g., Pod, Deployment, StatefulSet).
#// It handles different paths to the container spec based on the resource kind.
containers[container] {
#    // For standard pod specs
    container := input.spec.containers[_]
}
containers[container] {
#    // For pod templates within deployments, statefulsets, etc.
    container := input.spec.template.spec.containers[_]
}

#// --- Policy: Disallow 'latest' tag ---
#// Rationale: Using the ':latest' tag for container images is considered an anti-pattern in production.
#// It makes deployments non-deterministic and hard to track, as the image referenced by 'latest' can change
#// unexpectedly. This can lead to inconsistent deployments and complicates rollbacks.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if any container uses an image with the ':latest' tag or no tag at all.
deny[msg] {
    some container
    containers[container]
    image := container.image

#    // Check if the image name ends with ':latest' or has no tag specified.
#    // An image name without a colon implies the default 'latest' tag.
    endswith(image, ":latest")
    msg := sprintf("Image '%s' in container '%s' uses the 'latest' tag. Production deployments must use immutable, specific image tags.", [image, container.name])
}

deny[msg] {
    some container
    containers[container]
    image := container.image
    not contains(image, ":")
    msg := sprintf("Image '%s' in container '%s' has no specified tag (defaults to 'latest'). Production deployments must use immutable, specific image tags.", [image, container.name])
}


#// --- Policy: Require CPU and Memory Limits ---
#// Rationale: Unbounded containers can consume excessive resources, leading to resource starvation
#// on the node and potentially causing instability for other applications or the node itself (a "noisy neighbor" problem).
#// Setting explicit CPU and memory limits is crucial for ensuring predictable performance and stability.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if a container is missing CPU or memory limits.
deny[msg] {
    some container
    containers[container]
    not container.resources.limits.cpu
    msg := sprintf("Container '%s' is missing CPU limits. All production containers must have CPU limits set to ensure resource fairness and cluster stability.", [container.name])
}

deny[msg] {
    some container
    containers[container]
    not container.resources.limits.memory
    msg := sprintf("Container '%s' is missing memory limits. All production containers must have memory limits set to prevent out-of-memory (OOM) kills and cluster instability.", [container.name])
}


#// --- Policy: Prohibit Privileged Containers ---
#// Rationale: Privileged containers have direct access to the host's kernel and devices.
#// This breaks the isolation boundary between the container and the host, posing a significant
#// security risk. A compromised privileged container could lead to a full host compromise.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if a container requests `privileged: true`.
deny[msg] {
    some container
    containers[container]
    container.securityContext.privileged == true
    msg := sprintf("Container '%s' is running in privileged mode. Privileged containers are a major security risk and are not allowed in production.", [container.name])
}


#// --- Policy: Disallow Host Network/PID ---
#// Rationale: Sharing the host's network or process ID (PID) namespace reduces container isolation.
#// `hostNetwork` gives the container access to the host's network interfaces, bypassing network policies.
#// `hostPID` allows the container to see all processes on the host, potentially exposing sensitive information.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if the pod spec uses `hostNetwork: true` or `hostPID: true`.
deny[msg] {
    input.spec.hostNetwork == true
    msg := "Using 'hostNetwork' is not allowed. It compromises network isolation and security."
}

deny[msg] {
    input.spec.template.spec.hostNetwork == true
    msg := "Using 'hostNetwork' is not allowed. It compromises network isolation and security."
}

deny[msg] {
    input.spec.hostPID == true
    msg := "Using 'hostPID' is not allowed. It exposes all host processes to the container."
}

deny[msg] {
    input.spec.template.spec.hostPID == true
    msg := "Using 'hostPID' is not allowed. It exposes all host processes to the container."
}

#// --- Policy: Prevent Running as Root User ---
#// Rationale: Running containers as a non-root user is a critical security best practice (Principle of Least Privilege).
#// If an attacker gains control of a process inside a container running as root, they have root privileges
#// within that container, making it easier to exploit other vulnerabilities.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if a container is configured to run as root (or not configured to run as non-root).
deny[msg] {
    some container
    containers[container]

#    // The policy fails if `runAsUser` is explicitly set to 0 or if `runAsNonRoot` is not explicitly set to true.
#    // This provides a "secure by default" stance.
    not container.securityContext.runAsNonRoot == true
    msg := sprintf("Container '%s' must not run as root. Set `securityContext.runAsNonRoot` to true.", [container.name])
}

deny[msg] {
    some container
    containers[container]
    container.securityContext.runAsUser == 0
    msg := sprintf("Container '%s' must not run as root (runAsUser is 0). Please use a non-zero user ID.", [container.name])
}


#// --- Policy: Enforce Read-Only Root Filesystem ---
#// Rationale: An immutable root filesystem prevents an attacker from modifying the container's application binaries or
#// configuration files. It forces a clear separation of ephemeral data (which should be written to a volume)
#// from the static container image content.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if a container's root filesystem is not set to be read-only.
deny[msg] {
    some container
    containers[container]
    not container.securityContext.readOnlyRootFilesystem == true
    msg := sprintf("Container '%s' must have a read-only root filesystem. Set `securityContext.readOnlyRootFilesystem` to true and use volume mounts for writable paths.", [container.name])
}


#// --- Policy: Require Liveness and Readiness Probes ---
#// Rationale: Probes are essential for Kubernetes to manage a workload's lifecycle effectively.
#// A `readinessProbe` tells the service when a pod is ready to accept traffic.
#// A `livenessProbe` tells the kubelet when to restart a container that has become unhealthy.
#// Without them, traffic could be sent to a non-functional pod, or a failed pod might never be restarted.
#// Contract:
#//   - Input: A Kubernetes resource manifest.
#//   - Output: A detailed message if a container is missing a liveness or readiness probe.
deny[msg] {
    some container
    containers[container]
    not container.readinessProbe
    msg := sprintf("Container '%s' is missing a readinessProbe. This is required to ensure traffic is only sent when the application is ready.", [container.name])
}

deny[msg] {
    some container
    containers[container]
    not container.livenessProbe
    msg := sprintf("Container '%s' is missing a livenessProbe. This is required for Kubernetes to know when to restart an unhealthy container.", [container.name])
}