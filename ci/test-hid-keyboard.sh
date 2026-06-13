#!/usr/bin/env bash
# E2E test driver: HID keyboard tracer bullet.
#
# Orchestrates the full end-to-end flow:
#   1. Build kernel if not cached
#   2. Build initramfs with E2E test script embedded
#   3. Cross-compile usbip-server and usbip-client to musl
#   4. Boot QEMU with shared test binaries
#   5. Report pass/fail
#
# Usage:
#   ./ci/test-hid-keyboard.sh
#
# Environment:
#   QEMU_TIMEOUT  — QEMU boot timeout in seconds (default: 120)
#   KERNEL_VERSION — kernel version override
#   CI            — set to "true" for CI-friendly output
#
# Exit codes:
#   0  — E2E TEST PASS
#   1  — setup/compilation failure
#   2  — assertion failure (test ran but failed)
#   3+ — infra failure

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CI="${CI:-false}"

# ---- Step 1: Build kernel ----------------------------------------------------
echo "[test-hid-keyboard] Step 1/4: Building kernel..."
"${SCRIPT_DIR}/build-kernel.sh"

# ---- Step 2: Build initramfs -------------------------------------------------
echo "[test-hid-keyboard] Step 2/4: Building initramfs..."
"${SCRIPT_DIR}/build-initramfs.sh"

# ---- Step 3: Cross-compile server and client ---------------------------------
echo "[test-hid-keyboard] Step 3/4: Cross-compiling server and client..."

# Ensure musl target is installed
if ! rustup target list --installed 2>/dev/null | grep -q "x86_64-unknown-linux-musl"; then
    echo "[test-hid-keyboard] Installing x86_64-unknown-linux-musl target..."
    rustup target add x86_64-unknown-linux-musl
fi

# For cross-compilation, the server depends on rusb (libusb C library).
# On macOS with the musl target, we need either:
#   1. A cross-compiled libusb for x86_64-linux-musl
#   2. Or use cargo-zigbuild which includes Zig's cross-toolchain
#   3. Or build in a Linux container
#
# Check for convenient cross-compilation tools
CROSS_TOOL=""
if command -v cargo-zigbuild &>/dev/null; then
    CROSS_TOOL="zigbuild"
    echo "[test-hid-keyboard] Using cargo-zigbuild for cross-compilation"
elif command -v cross &>/dev/null; then
    CROSS_TOOL="cross"
    echo "[test-hid-keyboard] Using cross (Docker-based) for cross-compilation"
fi

# Cross-compile both binaries
case "${CROSS_TOOL}" in
    zigbuild)
        cargo zigbuild --release \
            --target x86_64-unknown-linux-musl \
            -p usbip-server \
            -p usbip-client
        ;;
    cross)
        cross build --release \
            --target x86_64-unknown-linux-musl \
            -p usbip-server \
            -p usbip-client
        ;;
    *)
        echo "[test-hid-keyboard] WARNING: No cross-compilation tool found."
        echo "[test-hid-keyboard] Attempting direct musl build (may fail on macOS without linker)."
        echo "[test-hid-keyboard] Install cargo-zigbuild or cross for reliable cross-compilation."
        cargo build --release \
            --target x86_64-unknown-linux-musl \
            -p usbip-server \
            -p usbip-client
        ;;
esac

# Verify binaries exist
SERVER_BIN="${PROJECT_DIR}/target/x86_64-unknown-linux-musl/release/usbip-server"
CLIENT_BIN="${PROJECT_DIR}/target/x86_64-unknown-linux-musl/release/usbip-client"

if [ ! -f "${SERVER_BIN}" ]; then
    echo "[test-hid-keyboard] ERROR: Server binary not found at ${SERVER_BIN}"
    exit 1
fi
if [ ! -f "${CLIENT_BIN}" ]; then
    echo "[test-hid-keyboard] ERROR: Client binary not found at ${CLIENT_BIN}"
    exit 1
fi

# Verify they're static binaries
FILE_OUTPUT=$(file "${SERVER_BIN}")
if ! echo "${FILE_OUTPUT}" | grep -q "static" && ! echo "${FILE_OUTPUT}" | grep -q "statically linked"; then
    echo "[test-hid-keyboard] WARNING: Server binary may not be fully static: ${FILE_OUTPUT}"
fi

FILE_OUTPUT=$(file "${CLIENT_BIN}")
if ! echo "${FILE_OUTPUT}" | grep -q "static" && ! echo "${FILE_OUTPUT}" | grep -q "statically linked"; then
    echo "[test-hid-keyboard] WARNING: Client binary may not be fully static: ${FILE_OUTPUT}"
fi

# Copy binaries to shared directory
SHARE_DIR="${SCRIPT_DIR}/test_binaries"
mkdir -p "${SHARE_DIR}"
cp "${SERVER_BIN}" "${SHARE_DIR}/usbip-server"
cp "${CLIENT_BIN}" "${SHARE_DIR}/usbip-client"
chmod +x "${SHARE_DIR}/usbip-server" "${SHARE_DIR}/usbip-client"
echo "[test-hid-keyboard] Copied binaries to ${SHARE_DIR}"

# ---- Step 4: Boot QEMU with E2E ---------------------------------------------
echo "[test-hid-keyboard] Step 4/4: Booting QEMU with E2E test..."
QEMU_LOG="${QEMU_LOG:-/tmp/qemu-e2e-log.txt}"
SMOKE_TEST_ONLY=1 \
    QEMU_LOG="${QEMU_LOG}" \
    QEMU_TIMEOUT="${QEMU_TIMEOUT:-120}" \
    "${SCRIPT_DIR}/boot-qemu.sh" --e2e

RESULT=$?

# ---- Step 5: Report ----------------------------------------------------------
if [ ${RESULT} -eq 0 ]; then
    if ${CI}; then
        echo "::notice title=E2E HID Keyboard::PASS"
    fi
    echo "[test-hid-keyboard] SUCCESS: HID keyboard E2E test passed."
    exit 0
else
    if ${CI}; then
        echo "::error title=E2E HID Keyboard::FAIL (exit ${RESULT})"
    fi
    echo "[test-hid-keyboard] FAILURE: HID keyboard E2E test failed (exit ${RESULT})."
    echo "[test-hid-keyboard] QEMU log: ${QEMU_LOG}"
    exit ${RESULT}
fi
