#!/bin/bash
# integration_test.sh - End-to-end test script for ph.

set -e # Exit immediately if a command fails.
set -o pipefail # Fail a pipeline if any command fails.

# --- Test Environment Setup ---
BUILD_DIR="build"
ph_BIN="$(pwd)/${BUILD_DIR}/bin/ph"
TEST_REPO_DIR="test_repo"
KIND_CLUSTER_NAME="ph-test-cluster"
KUBECONFIG_PATH="$(pwd)/kind-kubeconfig.yaml"

# --- Helper Functions ---
function print_header() {
    echo ""
    echo "--- $1 ---"
}

function cleanup() {
    print_header "Performing Cleanup"
    
    print_header "Stopping kind cluster"
    # Use sudo because the test script might be run with it
    sudo kind delete cluster --name "${KIND_CLUSTER_NAME}" || true
    
    print_header "Removing test directories"
    rm -rf "${TEST_REPO_DIR}"
    rm -f "${KUBECONFIG_PATH}"
    
    echo "Cleanup complete."
}

# Trap EXIT signal to ensure cleanup runs
trap cleanup EXIT

# --- Build Project ---
print_header "Building project"
# Use sudo for build to avoid permission issues with docker group
sudo cmake -S . -B "${BUILD_DIR}"
sudo cmake --build "${BUILD_DIR}" --parallel

# --- Basic Git Command Tests ---
print_header "Running Basic Git Command Tests"
rm -rf "${TEST_REPO_DIR}"
mkdir "${TEST_REPO_DIR}"
cd "${TEST_REPO_DIR}"

# Test Case 1: 'status' command on a clean repo
echo "--- Testing 'status' on a clean repo ---"
git init -b main
output=$("${ph_BIN}" status)
if [[ ! "$output" == *"Working tree clean"* ]]; then
    echo "FAIL: 'status' command did not report a clean working tree."
    exit 1
fi
echo "PASS: 'status' command works correctly on a clean repo."

# Test Case 2: 'SND' command workflow
echo "--- Testing 'SND' command ---"
echo "test content" > new_file.txt
"${ph_BIN}" SND
git log --oneline | grep "Automated commit from ph"
echo "PASS: 'SND' command created a commit."
cd ..

# --- Kubernetes Integration Tests ---
print_header "Starting Kubernetes Integration Tests"

# Check for dependencies
if ! command -v kind &> /dev/null || ! command -v kubectl &> /dev/null; then
    echo "WARNING: 'kind' or 'kubectl' not found. Skipping Kubernetes tests."
    exit 0
fi

# Setup kind cluster
print_header "Creating kind cluster: ${KIND_CLUSTER_NAME}"
sudo kind create cluster --name "${KIND_CLUSTER_NAME}" --kubeconfig "${KUBECONFIG_PATH}"
export KUBECONFIG="${KUBECONFIG_PATH}"
sudo chmod 644 "${KUBECONFIG_PATH}" # Allow kubectl to read the file
kubectl cluster-info

# The operator is not running in the cluster for these tests.
# We are testing the CLI's ability to interact with the cluster by calling
# the Rust FFI functions directly.

# Test Case 3: Successful 'kube rollout start'
print_header "Testing successful 'kube rollout start'"
kubectl create namespace ph-releases || true
"${ph_BIN}" kube rollout start --type Canary --app my-app --image my-image:v1.0 --skip-sig-check
kubectl get phgitreleases.ph.io -n ph-releases my-app -o jsonpath='{.spec.artifacts[0].image}' | grep "my-image:v1.0"
echo "PASS: 'kube rollout start' created the PhgitRelease resource."
kubectl delete phgitreleases.ph.io -n ph-releases my-app

# Test Case 4: Rollout blocked by signature verification
print_header "Testing rollout blocked by failed signature verification"
echo "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE...
-----END PUBLIC KEY-----" > cosign.pub
if "${ph_BIN}" kube rollout start --type Canary --app my-app-unsigned --image unsigned-image:v1.0 --public-key-file cosign.pub; then
    echo "FAIL: Rollout command succeeded for an unsigned image when verification was enabled."
    exit 1
fi
echo "PASS: 'kube rollout start' correctly failed for an unsigned image."
rm cosign.pub

# Test Case 5: Manual DR Failover Test
print_header "Testing manual DR failover"
# This test requires a running operator, which we are not doing here.
# This test is a placeholder to show the intent.
# A more advanced test framework would be needed to run the operator and test this.
echo "SKIPPING: Manual DR Failover test requires a running operator."


# --- Operator Integration Tests ---
print_header "Starting Operator Integration Tests"

# Build and load the operator image
print_header "Building operator Docker image"
sudo docker build -t ph-operator:test -f k8s/operators/ph_operator/Dockerfile k8s/operators/ph_operator
sudo kind load docker-image ph-operator:test --name "${KIND_CLUSTER_NAME}"

# Deploy the operator
print_header "Deploying ph-operator"
kubectl apply -f k8s/crd/
kubectl apply -f tests/operator-rbac.yaml
kubectl apply -f tests/operator-deployment.yaml

# Wait for the operator to be ready
print_header "Waiting for operator to become ready"
kubectl wait --for=condition=available --timeout=120s deployment/ph-operator-controller-manager -n ph-operator-system

# Test Case 6: Canary Rollback on Failure
print_header "Testing Canary Rollback on Failure"
# This test requires a metric that will fail. We'll use a dummy query
# that returns '0' and a success condition of 'result > 1'.
cat <<EOF | kubectl apply -f -
apiVersion: ph.io/v1alpha1
kind: phRelease
metadata:
  name: test-rollback
  namespace: default
spec:
  appName: failing-app
  version: "v1.0.1"
  strategy:
    type: Canary
    canary:
      trafficPercent: 50
      autoPromote: true
      analysis:
        interval: "5s"
        threshold: 2
        maxFailures: 2
        metrics:
        - name: dummy-failure-metric
          query: 'vector(0)'
          onSuccess: 'result > 1'
EOF

# Wait for the release to fail
echo "Waiting for the release to enter the 'Failed' phase..."
for _ in {1..30}; do
  phase=$(kubectl get phgitrelease/test-rollback -n default -o jsonpath='{.status.phase}' || echo "NotFound")
  if [[ "$phase" == "Failed" ]]; then
    echo "SUCCESS: Release 'test-rollback' entered 'Failed' phase as expected."
    break
  fi
  echo "Current phase: $phase. Waiting..."
  sleep 2
done

if [[ "$phase" != "Failed" ]]; then
  echo "FAIL: Release 'test-rollback' did not enter 'Failed' phase."
  kubectl get phgitrelease/test-rollback -n default -o yaml
  exit 1
fi
kubectl delete phgitrelease/test-rollback -n default

# Test Case 7: Preview Failure on Invalid Image
print_header "Testing Preview Failure on Invalid Image"
cat <<EOF | kubectl apply -f -
apiVersion: ph.io/v1alpha1
kind: phPreview
metadata:
  name: test-preview-failure
  namespace: default
spec:
  repoUrl: "https://github.com/phkaiser13/peitch-example-app"
  branch: "main"
  manifestPath: "k8s"
  appName: "invalid-image-app"
EOF

# The image name in the example repo is nginx, let's patch it to something invalid
# We need to wait for the namespace to be created first by the preview controller
echo "Waiting for preview namespace to be created..."
for _ in {1..15}; do
  ns=$(kubectl get phpreview/test-preview-failure -n default -o jsonpath='{.status.namespace}')
  if [[ -n "$ns" ]]; then
    echo "Preview namespace '$ns' found."
    break
  fi
  sleep 2
done

if [[ -z "$ns" ]]; then
  echo "FAIL: Preview namespace was not created."
  exit 1
fi

kubectl patch deployment/invalid-image-app -n "$ns" -p '{"spec":{"template":{"spec":{"containers":[{"name":"invalid-image-app","image":"invalid-repo/invalid-image:latest"}]}}}}'

echo "Waiting for preview to enter 'Failed' state due to ImagePullBackOff..."
for _ in {1..30}; do
  status_type=$(kubectl get phpreview/test-preview-failure -n default -o jsonpath='{.status.conditions[-1:].type}')
  status_msg=$(kubectl get phpreview/test-preview-failure -n default -o jsonpath='{.status.conditions[-1:].message}')
  if [[ "$status_type" == "Failed" && "$status_msg" == *"ImagePullBackOff"* ]]; then
    echo "SUCCESS: Preview 'test-preview-failure' failed with ImagePullBackOff as expected."
    break
  fi
  echo "Current status: $status_type. Waiting..."
  sleep 3
done

if [[ "$status_type" != "Failed" ]]; then
  echo "FAIL: Preview did not enter 'Failed' state as expected."
  kubectl get phpreview/test-preview-failure -n default -o yaml
  kubectl get pods -n "$ns"
  kubectl describe pod -l app=invalid-image-app -n "$ns"
  exit 1
fi
kubectl delete phpreview/test-preview-failure -n default


# --- Final success message ---
print_header "All integration tests passed!"
exit 0
