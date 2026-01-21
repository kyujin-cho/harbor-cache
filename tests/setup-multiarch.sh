#!/bin/bash
#
# Setup multi-architecture test images in Harbor
#
# This script pulls a multi-arch image from Docker Hub and pushes it to
# the local Harbor registry for testing purposes.
#
# Prerequisites:
#   - Docker installed with experimental features enabled (for manifest inspect)
#   - Harbor running at localhost:8880
#   - Docker logged into Harbor: docker login localhost:8880
#

set -e

HARBOR_URL="${HARBOR_URL:-localhost:8880}"
HARBOR_USER="${HARBOR_USER:-admin}"
HARBOR_PASS="${HARBOR_PASS:-Harbor12345}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_docker() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed"
        exit 1
    fi

    # Check if docker daemon is running
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running"
        exit 1
    fi
}

# Login to Harbor
login_harbor() {
    log_info "Logging into Harbor at ${HARBOR_URL}..."
    echo "${HARBOR_PASS}" | docker login "${HARBOR_URL}" -u "${HARBOR_USER}" --password-stdin
}

# Push a multi-arch image using regctl/crane or docker buildx
push_multiarch_with_crane() {
    local source_image="$1"
    local target_image="$2"

    # Check if crane is available
    if command -v crane &> /dev/null; then
        log_info "Using crane to copy multi-arch image..."
        crane copy "${source_image}" "${target_image}" --insecure
        return $?
    fi

    # Check if regctl is available
    if command -v regctl &> /dev/null; then
        log_info "Using regctl to copy multi-arch image..."
        regctl image copy "${source_image}" "${target_image}" --insecure
        return $?
    fi

    return 1
}

# Push multi-arch image using skopeo
push_multiarch_with_skopeo() {
    local source_image="$1"
    local target_image="$2"

    if command -v skopeo &> /dev/null; then
        log_info "Using skopeo to copy multi-arch image..."
        skopeo copy --all --dest-tls-verify=false \
            "docker://${source_image}" \
            "docker://${target_image}"
        return $?
    fi

    return 1
}

# Push multi-arch image using docker buildx
push_multiarch_with_buildx() {
    local source_image="$1"
    local target_image="$2"

    # Note: This method doesn't preserve original multi-arch, just creates new one
    log_info "docker buildx imagetools is not suitable for copying existing multi-arch images"
    return 1
}

# Main function to push multi-arch image
push_multiarch_image() {
    local source_image="$1"
    local target_image="$2"

    log_info "Copying ${source_image} -> ${target_image}"

    # Try different methods
    if push_multiarch_with_crane "${source_image}" "${target_image}"; then
        return 0
    fi

    if push_multiarch_with_skopeo "${source_image}" "${target_image}"; then
        return 0
    fi

    # Fallback: Pull and push individual architectures, then create manifest list
    log_info "Falling back to manual multi-arch push..."
    push_multiarch_manual "${source_image}" "${target_image}"
}

# Manually create multi-arch image by pulling each arch and creating manifest list
push_multiarch_manual() {
    local source_image="$1"
    local target_image="$2"

    # Pull source image for each architecture
    local platforms=("linux/amd64" "linux/arm64")
    local digests=()

    for platform in "${platforms[@]}"; do
        local arch_tag="${target_image}-$(echo ${platform} | tr '/' '-')"

        log_info "Pulling ${source_image} for ${platform}..."
        if docker pull --platform "${platform}" "${source_image}" 2>/dev/null; then
            log_info "Tagging and pushing for ${platform}..."
            docker tag "${source_image}" "${arch_tag}"
            docker push "${arch_tag}"

            # Get the digest
            local digest=$(docker inspect --format='{{index .RepoDigests 0}}' "${arch_tag}" | cut -d'@' -f2)
            digests+=("${target_image}@${digest}")

            # Clean up
            docker rmi "${arch_tag}" 2>/dev/null || true
        else
            log_info "Platform ${platform} not available for ${source_image}"
        fi
    done

    # Create and push manifest list
    if [ ${#digests[@]} -gt 1 ]; then
        log_info "Creating manifest list with ${#digests[@]} architectures..."

        # Remove existing manifest if any
        docker manifest rm "${target_image}" 2>/dev/null || true

        # Create new manifest list
        docker manifest create "${target_image}" "${digests[@]}"

        # Push manifest list
        docker manifest push "${target_image}"

        log_success "Multi-arch image pushed: ${target_image}"
    elif [ ${#digests[@]} -eq 1 ]; then
        log_info "Only one architecture available, pushing single-arch image"
        docker tag "${source_image}" "${target_image}"
        docker push "${target_image}"
    else
        log_error "No architectures available for ${source_image}"
        return 1
    fi
}

# Setup function
setup_test_images() {
    check_docker
    login_harbor

    # Images to setup for testing
    # Using nginx:alpine as it commonly has multi-arch support
    local test_images=(
        "nginx:alpine-slim:library/nginx:alpine"
        "alpine:3.19:library/alpine-multiarch:latest"
    )

    for image_spec in "${test_images[@]}"; do
        local source=$(echo "${image_spec}" | cut -d: -f1-2)
        local target="${HARBOR_URL}/$(echo "${image_spec}" | cut -d: -f3-4)"

        log_info "Setting up ${source} -> ${target}"
        push_multiarch_image "${source}" "${target}" || {
            log_error "Failed to push ${source}"
        }
    done

    log_success "Test image setup complete!"
    echo ""
    log_info "You can now run: ./tests/e2e-test.sh multiarch"
}

# Alternative: Simple setup with skopeo (recommended)
simple_setup() {
    log_info "Simple setup using direct API calls..."

    # First, check if skopeo is available (best tool for this)
    if ! command -v skopeo &> /dev/null; then
        log_error "skopeo is not installed"
        log_info "Install with: brew install skopeo (macOS) or apt install skopeo (Linux)"
        log_info ""
        log_info "Alternative: Install crane from https://github.com/google/go-containerregistry"
        exit 1
    fi

    # Copy nginx:alpine (multi-arch) to local Harbor
    log_info "Copying nginx:alpine-slim to Harbor..."
    skopeo copy --all --dest-tls-verify=false \
        docker://docker.io/library/nginx:alpine-slim \
        docker://${HARBOR_URL}/library/nginx:alpine

    log_success "nginx:alpine (multi-arch) pushed to Harbor"

    # Copy alpine:3.19 (multi-arch) to local Harbor
    log_info "Copying alpine:3.19 to Harbor..."
    skopeo copy --all --dest-tls-verify=false \
        docker://docker.io/library/alpine:3.19 \
        docker://${HARBOR_URL}/library/alpine:multiarch

    log_success "alpine:multiarch pushed to Harbor"

    echo ""
    log_success "Multi-arch test images are ready!"
    log_info "Run tests with: ./tests/e2e-test.sh multiarch"
}

# Print usage
usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  setup    - Setup test images using available tools"
    echo "  simple   - Simple setup using skopeo (recommended)"
    echo "  help     - Show this help"
    echo ""
    echo "Prerequisites:"
    echo "  - Harbor running at localhost:8880"
    echo "  - One of: skopeo, crane, or docker with buildx"
}

# Main
case "${1:-setup}" in
    setup)
        setup_test_images
        ;;
    simple)
        simple_setup
        ;;
    help|--help|-h)
        usage
        ;;
    *)
        usage
        exit 1
        ;;
esac
