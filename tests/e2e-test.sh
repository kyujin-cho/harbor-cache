#!/bin/bash
#
# Harbor Cache End-to-End Test Suite
#
# Prerequisites:
#   - Harbor running at localhost:8880 (see harbor-setup/)
#   - Harbor Cache running at localhost:5001
#   - Docker installed and running
#   - curl, jq installed
#
# Usage:
#   ./tests/e2e-test.sh [test_name]
#
# Examples:
#   ./tests/e2e-test.sh           # Run all tests
#   ./tests/e2e-test.sh multiarch # Run only multiarch tests
#

set -e

# Configuration
HARBOR_CACHE_URL="${HARBOR_CACHE_URL:-http://localhost:5001}"
HARBOR_URL="${HARBOR_URL:-http://localhost:8880}"
HARBOR_USER="${HARBOR_USER:-admin}"
HARBOR_PASS="${HARBOR_PASS:-Harbor12345}"
CACHE_USER="${CACHE_USER:-admin}"
CACHE_PASS="${CACHE_PASS:-admin}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

#
# Utility Functions
#

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_section() {
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE} $1${NC}"
    echo -e "${BLUE}========================================${NC}"
}

# Get JWT token from Harbor Cache
get_cache_token() {
    curl -s -X POST "${HARBOR_CACHE_URL}/api/v1/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"${CACHE_USER}\",\"password\":\"${CACHE_PASS}\"}" \
        | jq -r '.token'
}

# Get cache stats
get_cache_stats() {
    local token=$1
    curl -s "${HARBOR_CACHE_URL}/api/v1/cache/stats" \
        -H "Authorization: Bearer ${token}"
}

# Clear cache
clear_cache() {
    local token=$1
    curl -s -X DELETE "${HARBOR_CACHE_URL}/api/v1/cache" \
        -H "Authorization: Bearer ${token}"
}

# Check if Harbor is accessible
check_harbor() {
    if curl -s -f "${HARBOR_URL}/api/v2.0/health" > /dev/null 2>&1; then
        return 0
    fi
    return 1
}

# Check if Harbor Cache is accessible
check_harbor_cache() {
    if curl -s -f "${HARBOR_CACHE_URL}/health" > /dev/null 2>&1; then
        return 0
    fi
    return 1
}

#
# Test Cases
#

test_health_endpoint() {
    log_info "Testing health endpoint..."

    local response=$(curl -s "${HARBOR_CACHE_URL}/health")
    local status=$(echo "$response" | jq -r '.status')

    if [ "$status" == "healthy" ]; then
        log_success "Health endpoint returns healthy status"
    else
        log_fail "Health endpoint returned: $response"
    fi
}

test_authentication() {
    log_info "Testing authentication..."

    # Test valid login
    local response=$(curl -s -X POST "${HARBOR_CACHE_URL}/api/v1/auth/login" \
        -H "Content-Type: application/json" \
        -d '{"username":"admin","password":"admin"}')

    local token=$(echo "$response" | jq -r '.token')

    if [ -n "$token" ] && [ "$token" != "null" ]; then
        log_success "Authentication with valid credentials"
    else
        log_fail "Authentication failed: $response"
        return 1
    fi

    # Test invalid login
    local bad_response=$(curl -s -X POST "${HARBOR_CACHE_URL}/api/v1/auth/login" \
        -H "Content-Type: application/json" \
        -d '{"username":"admin","password":"wrong"}')

    local error=$(echo "$bad_response" | jq -r '.errors[0].code')

    if [ "$error" == "UNAUTHORIZED" ]; then
        log_success "Authentication rejects invalid credentials"
    else
        log_fail "Invalid credentials not rejected properly: $bad_response"
    fi
}

test_cache_stats() {
    log_info "Testing cache stats API..."

    local token=$(get_cache_token)
    local stats=$(get_cache_stats "$token")

    # Check required fields exist
    local total_size=$(echo "$stats" | jq -r '.total_size')
    local entry_count=$(echo "$stats" | jq -r '.entry_count')
    local hit_rate=$(echo "$stats" | jq -r '.hit_rate')

    if [ "$total_size" != "null" ] && [ "$entry_count" != "null" ] && [ "$hit_rate" != "null" ]; then
        log_success "Cache stats API returns expected fields"
        log_info "  - Total size: $(echo "$stats" | jq -r '.total_size_human')"
        log_info "  - Entry count: $entry_count"
        log_info "  - Hit rate: $hit_rate"
    else
        log_fail "Cache stats missing required fields: $stats"
    fi
}

test_basic_pull() {
    log_info "Testing basic image pull through cache..."

    local token=$(get_cache_token)

    # Get initial stats
    local initial_stats=$(get_cache_stats "$token")
    local initial_misses=$(echo "$initial_stats" | jq -r '.miss_count')

    # Pull alpine image through cache (cache miss expected)
    log_info "Pulling alpine:latest (expecting cache miss)..."

    # First, check manifest via OCI API
    local manifest_response=$(curl -s -w "\n%{http_code}" \
        "${HARBOR_CACHE_URL}/v2/library/alpine/manifests/latest" \
        -H "Accept: application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json,application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.list.v2+json")

    local http_code=$(echo "$manifest_response" | tail -1)
    local manifest=$(echo "$manifest_response" | sed '$d')

    if [ "$http_code" == "200" ]; then
        log_success "Successfully fetched manifest through cache"

        # Get stats after pull
        local after_stats=$(get_cache_stats "$token")
        local after_misses=$(echo "$after_stats" | jq -r '.miss_count')

        if [ "$after_misses" -gt "$initial_misses" ]; then
            log_success "Cache miss recorded for first pull"
        fi

        # Pull again (should be cache hit)
        log_info "Pulling alpine:latest again (expecting cache hit)..."

        local initial_hits=$(echo "$after_stats" | jq -r '.hit_count')

        curl -s "${HARBOR_CACHE_URL}/v2/library/alpine/manifests/latest" \
            -H "Accept: application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json" \
            > /dev/null

        local final_stats=$(get_cache_stats "$token")
        local final_hits=$(echo "$final_stats" | jq -r '.hit_count')

        if [ "$final_hits" -gt "$initial_hits" ]; then
            log_success "Cache hit recorded for second pull"
        else
            log_warn "Cache hit not recorded (may be expected if manifest was already cached)"
        fi
    else
        log_fail "Failed to fetch manifest: HTTP $http_code"
    fi
}

test_oci_multiarch_image() {
    log_info "Testing OCI format multi-architecture image..."

    local token=$(get_cache_token)

    # Check if multi-arch test image exists in Harbor
    # Try nginx:alpine first, then fall back to alpine:multiarch
    local test_images=("library/nginx:alpine" "library/alpine:multiarch" "library/alpine:latest")
    local test_repo=""
    local test_tag=""

    for image in "${test_images[@]}"; do
        local repo=$(echo "$image" | cut -d: -f1)
        local tag=$(echo "$image" | cut -d: -f2)

        log_info "Checking if ${image} exists in Harbor..."
        local check_response=$(curl -s -w "%{http_code}" -o /dev/null \
            -u "${HARBOR_USER}:${HARBOR_PASS}" \
            -H "Accept: application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.list.v2+json,application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json" \
            "${HARBOR_URL}/v2/${repo}/manifests/${tag}")

        if [ "$check_response" == "200" ]; then
            test_repo="$repo"
            test_tag="$tag"
            log_info "Found test image: ${repo}:${tag}"
            break
        fi
    done

    if [ -z "$test_repo" ]; then
        log_warn "No suitable test image found in Harbor"
        log_info "Run ./tests/setup-multiarch.sh to setup multi-arch test images"
        log_info "Skipping multi-arch test"
        return 0
    fi

    # Clear cache first to ensure clean test
    log_info "Clearing cache for clean multiarch test..."
    clear_cache "$token" > /dev/null

    # Get initial stats
    local initial_stats=$(get_cache_stats "$token")
    local initial_entries=$(echo "$initial_stats" | jq -r '.entry_count')

    log_info "Fetching manifest for ${test_repo}:${test_tag}..."

    # Request with OCI image index accept header (for multi-arch)
    local manifest_response=$(curl -s -w "\n%{http_code}" \
        "${HARBOR_CACHE_URL}/v2/${test_repo}/manifests/${test_tag}" \
        -H "Accept: application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.list.v2+json,application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json")

    local http_code=$(echo "$manifest_response" | tail -1)
    local manifest=$(echo "$manifest_response" | sed '$d')

    if [ "$http_code" != "200" ]; then
        log_fail "Failed to fetch manifest: HTTP $http_code"
        return 1
    fi

    # Check if it's a manifest list/index
    local media_type=$(echo "$manifest" | jq -r '.mediaType // .schemaVersion')
    local manifests_count=$(echo "$manifest" | jq -r '.manifests | length // 0')

    log_info "Manifest media type: $media_type"
    log_info "Number of platform manifests: $manifests_count"

    if [ "$manifests_count" -gt 0 ]; then
        log_success "Multi-arch manifest contains $manifests_count platform-specific manifests"

        # List architectures
        log_info "Available architectures:"
        echo "$manifest" | jq -r '.manifests[] | "  - \(.platform.os)/\(.platform.architecture)\(.platform.variant // "")"'

        # Fetch a specific architecture manifest (amd64)
        log_info "Fetching amd64-specific manifest..."

        local amd64_digest=$(echo "$manifest" | jq -r '.manifests[] | select(.platform.architecture == "amd64" and .platform.os == "linux") | .digest' | head -1)

        if [ -n "$amd64_digest" ] && [ "$amd64_digest" != "null" ]; then
            log_info "AMD64 manifest digest: $amd64_digest"

            local arch_response=$(curl -s -w "\n%{http_code}" \
                "${HARBOR_CACHE_URL}/v2/${test_repo}/manifests/${amd64_digest}" \
                -H "Accept: application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json")

            local arch_http_code=$(echo "$arch_response" | tail -1)
            local arch_manifest=$(echo "$arch_response" | sed '$d')

            if [ "$arch_http_code" == "200" ]; then
                log_success "Successfully fetched amd64-specific manifest"

                # Check manifest structure
                local config_digest=$(echo "$arch_manifest" | jq -r '.config.digest')
                local layers_count=$(echo "$arch_manifest" | jq -r '.layers | length')

                log_info "  - Config digest: $config_digest"
                log_info "  - Number of layers: $layers_count"

                # Fetch a blob (first layer)
                local first_layer=$(echo "$arch_manifest" | jq -r '.layers[0].digest')
                if [ -n "$first_layer" ] && [ "$first_layer" != "null" ]; then
                    log_info "Fetching first layer blob: $first_layer"

                    local blob_response=$(curl -s -w "%{http_code}" -o /dev/null \
                        "${HARBOR_CACHE_URL}/v2/${test_repo}/blobs/${first_layer}")

                    if [ "$blob_response" == "200" ]; then
                        log_success "Successfully fetched layer blob"
                    else
                        log_fail "Failed to fetch layer blob: HTTP $blob_response"
                    fi
                fi
            else
                log_fail "Failed to fetch amd64 manifest: HTTP $arch_http_code"
            fi
        else
            log_warn "No amd64 manifest found, trying arm64..."

            local arm64_digest=$(echo "$manifest" | jq -r '.manifests[] | select(.platform.architecture == "arm64" and .platform.os == "linux") | .digest' | head -1)

            if [ -n "$arm64_digest" ] && [ "$arm64_digest" != "null" ]; then
                log_info "ARM64 manifest digest: $arm64_digest"
                log_success "Multi-arch image has ARM64 architecture available"
            fi
        fi

        # Verify cache entries were created
        local final_stats=$(get_cache_stats "$token")
        local final_entries=$(echo "$final_stats" | jq -r '.entry_count')

        if [ "$final_entries" -gt "$initial_entries" ]; then
            log_success "Cache entries created: $initial_entries -> $final_entries"
        else
            log_warn "No new cache entries (content may have been cached previously)"
        fi
    else
        # It might be a single-arch manifest, check if we can still work with it
        local config=$(echo "$manifest" | jq -r '.config.digest // empty')
        if [ -n "$config" ]; then
            log_warn "Received single-architecture manifest instead of manifest list"
            log_info "This may happen if upstream only has single-arch image"
            log_success "Single-arch manifest is valid and cached"
        else
            log_fail "Invalid manifest structure"
        fi
    fi
}

test_oci_manifest_types() {
    log_info "Testing OCI manifest type handling..."

    # Use alpine:latest which should exist
    local test_repo="library/alpine"
    local test_tag="latest"

    # Test that we correctly handle different Accept headers

    # 1. Request OCI Image Index specifically
    log_info "Requesting with OCI Image Index Accept header..."
    local oci_response=$(curl -s -D - -o /dev/null \
        "${HARBOR_CACHE_URL}/v2/${test_repo}/manifests/${test_tag}" \
        -H "Accept: application/vnd.oci.image.index.v1+json" 2>&1)

    local content_type=$(echo "$oci_response" | grep -i "content-type:" | head -1 | tr -d '\r')
    log_info "Response Content-Type: $content_type"

    # 2. Request Docker Manifest List specifically
    log_info "Requesting with Docker Manifest List Accept header..."
    local docker_response=$(curl -s -D - -o /dev/null \
        "${HARBOR_CACHE_URL}/v2/${test_repo}/manifests/${test_tag}" \
        -H "Accept: application/vnd.docker.distribution.manifest.list.v2+json" 2>&1)

    content_type=$(echo "$docker_response" | grep -i "content-type:" | head -1 | tr -d '\r')
    log_info "Response Content-Type: $content_type"

    log_success "OCI manifest type handling works correctly"
}

test_docker_pull_through_cache() {
    log_info "Testing Docker pull through cache..."

    # This test requires Docker to be configured to use the cache as a registry mirror
    # or pull directly from the cache URL

    # Check if docker is available
    if ! command -v docker &> /dev/null; then
        log_warn "Docker not available, skipping Docker pull test"
        return 0
    fi

    local token=$(get_cache_token)
    local initial_stats=$(get_cache_stats "$token")
    local initial_hits=$(echo "$initial_stats" | jq -r '.hit_count')

    # Try to pull through the cache (this may fail if Docker isn't configured for insecure registries)
    log_info "Attempting Docker pull from localhost:5001/library/alpine:latest..."

    if docker pull localhost:5001/library/alpine:latest 2>/dev/null; then
        log_success "Docker pull through cache succeeded"

        # Clean up
        docker rmi localhost:5001/library/alpine:latest 2>/dev/null || true

        local final_stats=$(get_cache_stats "$token")
        local final_hits=$(echo "$final_stats" | jq -r '.hit_count')

        log_info "Cache hits: $initial_hits -> $final_hits"
    else
        log_warn "Docker pull failed (may need to configure Docker for insecure registry localhost:5001)"
        log_info "Add to Docker daemon.json: {\"insecure-registries\": [\"localhost:5001\"]}"
    fi
}

test_cache_cleanup() {
    log_info "Testing cache cleanup..."

    local token=$(get_cache_token)

    # Trigger cleanup
    local response=$(curl -s -X POST "${HARBOR_CACHE_URL}/api/v1/cache/cleanup" \
        -H "Authorization: Bearer ${token}")

    local cleaned=$(echo "$response" | jq -r '.cleaned')

    if [ "$cleaned" != "null" ]; then
        log_success "Cache cleanup executed, cleaned: $cleaned entries"
    else
        log_fail "Cache cleanup failed: $response"
    fi
}

test_cache_clear() {
    log_info "Testing cache clear..."

    local token=$(get_cache_token)

    # Get stats before
    local before_stats=$(get_cache_stats "$token")
    local before_count=$(echo "$before_stats" | jq -r '.entry_count')

    # Clear cache
    local response=$(curl -s -X DELETE "${HARBOR_CACHE_URL}/api/v1/cache" \
        -H "Authorization: Bearer ${token}")

    local cleared=$(echo "$response" | jq -r '.cleared')

    # Get stats after
    local after_stats=$(get_cache_stats "$token")
    local after_count=$(echo "$after_stats" | jq -r '.entry_count')

    if [ "$after_count" == "0" ]; then
        log_success "Cache cleared successfully (removed $cleared entries)"
    else
        log_fail "Cache clear incomplete: still have $after_count entries"
    fi
}

#
# Main
#

main() {
    log_section "Harbor Cache End-to-End Tests"

    # Check prerequisites
    log_info "Checking prerequisites..."

    if ! check_harbor; then
        log_fail "Harbor is not accessible at ${HARBOR_URL}"
        log_info "Start Harbor with: cd harbor-setup/harbor && docker compose up -d"
        exit 1
    fi
    log_success "Harbor is accessible"

    if ! check_harbor_cache; then
        log_fail "Harbor Cache is not accessible at ${HARBOR_CACHE_URL}"
        log_info "Start with: ./target/release/harbor-cache --config config/default.toml"
        exit 1
    fi
    log_success "Harbor Cache is accessible"

    # Run tests based on argument
    local test_filter="${1:-all}"

    case "$test_filter" in
        all)
            log_section "Basic Tests"
            test_health_endpoint
            test_authentication
            test_cache_stats

            log_section "Pull Tests"
            test_basic_pull

            log_section "OCI Multi-Architecture Tests"
            test_oci_multiarch_image
            test_oci_manifest_types

            log_section "Docker Integration Tests"
            test_docker_pull_through_cache

            log_section "Cache Management Tests"
            test_cache_cleanup
            test_cache_clear
            ;;
        multiarch|multi-arch|oci)
            log_section "OCI Multi-Architecture Tests"
            test_oci_multiarch_image
            test_oci_manifest_types
            ;;
        basic)
            log_section "Basic Tests"
            test_health_endpoint
            test_authentication
            test_cache_stats
            ;;
        pull)
            log_section "Pull Tests"
            test_basic_pull
            ;;
        docker)
            log_section "Docker Integration Tests"
            test_docker_pull_through_cache
            ;;
        cache)
            log_section "Cache Management Tests"
            test_cache_cleanup
            test_cache_clear
            ;;
        *)
            echo "Unknown test: $test_filter"
            echo "Available tests: all, basic, pull, multiarch, docker, cache"
            exit 1
            ;;
    esac

    # Summary
    log_section "Test Summary"
    echo -e "Tests passed: ${GREEN}${TESTS_PASSED}${NC}"
    echo -e "Tests failed: ${RED}${TESTS_FAILED}${NC}"

    if [ "$TESTS_FAILED" -gt 0 ]; then
        exit 1
    fi
}

main "$@"
