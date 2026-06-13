#!/usr/bin/env bash
# Build a minimal Linux kernel bzImage for USB/IP CI smoke tests.
#
# Usage:
#   ./ci/build-kernel.sh                  # build with default version
#   KERNEL_VERSION=6.6.50 ./ci/build-kernel.sh  # override version
#
# Output: ci/kernel/bzImage
# Cache:  skips rebuild if ci/kernel/bzImage exists and is newer than
#         ci/kernel/config.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KERNEL_DIR="${SCRIPT_DIR}/kernel"
OUTPUT="${KERNEL_DIR}/bzImage"
CONFIG_SRC="${KERNEL_DIR}/config"
KERNEL_VERSION="${KERNEL_VERSION:-6.6.50}"
BUILD_BASE="/tmp"

# ---- Cache check -----------------------------------------------------------
if [[ -f "${OUTPUT}" && "${CONFIG_SRC}" -ot "${OUTPUT}" ]]; then
    echo "[build-kernel] bzImage is up to date (older than config), skipping build."
    exit 0
fi

BUILD_DIR="${BUILD_BASE}/kernel-build-${KERNEL_VERSION}"
KERNEL_TAR_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
KERNEL_TAR="${BUILD_BASE}/linux-${KERNEL_VERSION}.tar.xz"

# ---- Download kernel source (cached) ---------------------------------------
if [[ ! -f "${KERNEL_TAR}" ]]; then
    echo "[build-kernel] Downloading Linux ${KERNEL_VERSION} from kernel.org..."
    curl -fsSL "${KERNEL_TAR_URL}" -o "${KERNEL_TAR}"
else
    echo "[build-kernel] Kernel tarball already cached at ${KERNEL_TAR}"
fi

# ---- Extract ---------------------------------------------------------------
if [[ ! -d "${BUILD_DIR}" ]]; then
    echo "[build-kernel] Extracting to ${BUILD_DIR}..."
    mkdir -p "${BUILD_DIR}"
    tar -xJf "${KERNEL_TAR}" -C "${BUILD_BASE}"
    # The tarball creates linux-<version>/, but we want BUILD_DIR to be that dir
    # If we created BUILD_DIR first, the extract lands alongside it; handle both layouts.
    EXTRACTED="${BUILD_BASE}/linux-${KERNEL_VERSION}"
    if [[ -d "${EXTRACTED}" ]]; then
        # If BUILD_DIR exists and is empty (we just created it), remove and rename
        if [[ -d "${BUILD_DIR}" ]] && [[ -z "$(ls -A "${BUILD_DIR}" 2>/dev/null)" ]]; then
            rmdir "${BUILD_DIR}"
        fi
        mv "${EXTRACTED}" "${BUILD_DIR}"
    fi
fi

# ---- Configure -------------------------------------------------------------
echo "[build-kernel] Applying kernel config..."
cp "${CONFIG_SRC}" "${BUILD_DIR}/.config"
cd "${BUILD_DIR}"

# Apply defaults for any options not specified in our fragment
make olddefconfig

# ---- Build -----------------------------------------------------------------
echo "[build-kernel] Building bzImage with $(nproc) parallel jobs..."
make -j"$(nproc)" bzImage

# ---- Copy artifact ---------------------------------------------------------
cp arch/x86/boot/bzImage "${OUTPUT}"
echo "[build-kernel] Done — bzImage written to ${OUTPUT}"
