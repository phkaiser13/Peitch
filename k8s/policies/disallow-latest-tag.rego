#/*
# * Copyright (C) 2025 Pedro Henrique / phkaiser13
# *
# * File: k8s/policies/disallow-latest-tag.rego
# *
# * This Rego policy file defines a specific, focused admission control rule for a Kubernetes
# * environment. Its sole purpose is to prevent the use of images with the ':latest' tag in
# * container specifications. This policy is a critical best practice for ensuring deployment
# * stability and predictability in production environments.
# *
# * The architecture is a simple 'deny' block that triggers if any container in a submitted
# * workload (e.g., Deployment, Pod, StatefulSet) uses an image name ending in ':latest'.
# * This approach enforces immutability; every change to a running application must be
# * accompanied by a manifest change that specifies a unique, immutable image tag (like a
# * version number or a git commit hash). This guarantees that rollbacks are predictable
# * and that the cluster's state can be perfectly replicated from its declarative configuration.
# *
# * While this logic is also part of the more comprehensive 'production-safeguards.rego' policy,
# * this file exists to provide a granular control that can be applied independently. For
# * instance, this rule might be enforced across all namespaces, including development and
# * staging, while other, more restrictive policies are only applied to production.
# *
# * SPDX-License-Identifier: Apache-2.0
# */
package kubernetes.policies.images

#
# ---[ Policy: Disallow 'latest' Tag ]---------------------------------------
#
# Deny any container image that uses the ':latest' tag.
#
# **Reasoning:** Using the ':latest' tag is an anti-pattern in any automated or
# production-grade system. It introduces ambiguity, as the underlying image digest
# for the tag can change without notice. This leads to several critical problems:
#
# 1.  **Non-deterministic Deployments:** Re-applying the same manifest can result in
# different container images being pulled, leading to unpredictable behavior.
# 2.  **Difficult Rollbacks:** If a bad image is pushed to ':latest', rolling back
# requires finding and deploying a previous, known-good tag. The manifest itself
# does not contain enough information to revert the change.
# 3.  **Breaks Immutability:** Kubernetes operates on a declarative model. The
# running state should reflect the declared state. Mutable tags break this contract.
#
# By enforcing the use of specific, immutable tags (e.g., `v1.2.3`, `commit-a1b2c3d`),
# we ensure that our deployments are reliable, repeatable, and auditable.
#
deny[msg] {
	# Check across all common workload types for container specs.
	container := input.request.object.spec.containers[_]
	endswith(container.image, ":latest")
	msg := sprintf("Image '%v' uses the mutable ':latest' tag. Please use a specific, immutable image tag (e.g., a version number or git hash).", [container.image])
}

deny[msg] {
	container := input.request.object.spec.template.spec.containers[_]
	endswith(container.image, ":latest")
	msg := sprintf("Image '%v' in pod template uses the mutable ':latest' tag. Please use a specific, immutable image tag (e.g., a version number or git hash).", [container.image])
}