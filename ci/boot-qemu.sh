#!/usr/bin/env bash
# Boot the USB/IP CI kernel + initramfs in QEMU and verify the smoke test
# or E2E test result.
#
# Usage:
#   ./ci/boot-qemu.sh                          # default run (smoke test)
#   ./ci/boot-qemu.sh --e2e                    # E2E test mode
#   QEMU_TIMEOUT=60 ./ci/boot-qemu.sh          # custom timeout (seconds)
#   QEMU_LOG=/tmp/qemu-ci.log ./ci/boot-qemu.sh  # custom log path
#
# Exit codes:
#   0  — SMOKE_TEST_PASS (default) or E2E_TEST_PASS (--e2e)
#   1  — setup failure (missing files, dependencies)
#   2  — assertion failure (test ran but failed)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KERNEL_DIR="${SCRIPT_DIR}/kernel"
BZIMAGE="${KERNEL_DIR}/bzImage"
INITRAMFS="${KERNEL_DIR}/initramfs.cpio.gz"
QEMU_LOG="${QEMU_LOG:-${SCRIPT_DIR}/kernel/qemu-serial.log}"
if [ "${CI:-}" = "true" ]; then
    QEMU_LOG="${SCRIPT_DIR}/kernel/qemu-serial.log"
fi
QEMU_TIMEOUT="${QEMU_TIMEOUT:-120}"

# Parse flags
E2E_MODE=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --e2e)
            E2E_MODE=1
            shift
            ;;
        *)
            echo "[boot-qemu] ERROR: Unknown option: $1" >&2
            echo "[boot-qemu] Usage: $0 [--e2e]" >&2
            exit 1
            ;;
    esac
done

# ---- Prerequisites ---------------------------------------------------------
for f in "${BZIMAGE}" "${INITRAMFS}"; do
    if [[ ! -f "${f}" ]]; then
        echo "[boot-qemu] ERROR: Missing ${f}" >&2
        echo "[boot-qemu] Run ci/build-kernel.sh and ci/build-initramfs.sh first." >&2
        exit 1
    fi
done

if ! command -v qemu-system-x86_64 &>/dev/null; then
    echo "[boot-qemu] ERROR: qemu-system-x86_64 not found. Install QEMU." >&2
    exit 1
fi

# ---- Accelerator detection -------------------------------------------------
if [[ -e /dev/kvm ]] && [[ -r /dev/kvm ]] && [[ -w /dev/kvm ]]; then
    ACCEL="-enable-kvm"
    echo "[boot-qemu] KVM available — enabling hardware acceleration"
else
    ACCEL="-accel tcg"
    echo "[boot-qemu] KVM not available — falling back to TCG (slow)"
fi

# ---- Serial output handling ------------------------------------------------
rm -f "${QEMU_LOG}"

SERIAL_OPTS="-nographic -serial file:${QEMU_LOG}"

# ---- Kernel command line ---------------------------------------------------
CMDLINE="console=ttyS0,115200 earlyprintk=serial quiet"

if [[ ${E2E_MODE} -eq 1 ]]; then
    CMDLINE="${CMDLINE} E2E_MODE=1"
    echo "[boot-qemu] E2E mode enabled — will check for E2E_TEST_PASS"
    SMOKE_MARKER="E2E_TEST_PASS"
    FAIL_MARKER="E2E_TEST_FAIL"
else
    SMOKE_MARKER="SMOKE_TEST_PASS"
    FAIL_MARKER="SMOKE_TEST_FAIL"
fi

# ---- Build QEMU command ----------------------------------------------------
QEMU_CMD=(
    qemu-system-x86_64
    ${ACCEL}
    ${SERIAL_OPTS}
    -m 512M
    -kernel "${BZIMAGE}"
    -initrd "${INITRAMFS}"
    -append "${CMDLINE}"
)

# Share test binaries directory if it exists
TEST_BINARIES_DIR="${SCRIPT_DIR}/test_binaries"
if [[ -d "${TEST_BINARIES_DIR}" ]]; then
    QEMU_CMD+=(
        -virtfs
        "local,path=${TEST_BINARIES_DIR},mount_tag=test_share,security_model=none"
    )
    echo "[boot-qemu] Sharing test binaries from ${TEST_BINARIES_DIR}"
fi

echo "[boot-qemu] Starting QEMU (timeout: ${QEMU_TIMEOUT}s)..."
echo "[boot-qemu] Log: ${QEMU_LOG}"
echo "[boot-qemu] Command: ${QEMU_CMD[*]}"

# ---- Run QEMU with timeout -------------------------------------------------
# shellcheck disable=SC2069
timeout "${QEMU_TIMEOUT}" "${QEMU_CMD[@]}" 2>/dev/null || true

# ---- Check results ---------------------------------------------------------
echo "[boot-qemu] QEMU exited. Checking log for test markers..."

if [[ ! -f "${QEMU_LOG}" ]]; then
    echo "[boot-qemu] ERROR: No QEMU log file found at ${QEMU_LOG}"
    exit 1
fi

# Extract key lines for CI output
EXTRACT_PATTERN="(CI Initramfs|===|Found UDC|E2E_|SMOKE_|Test suite|GADGET_|SERVER_|DEVLIST_|IMPORT_|FAIL:|BUSID)"

if grep -q "${SMOKE_MARKER}" "${QEMU_LOG}"; then
    echo "[boot-qemu] PASS — ${SMOKE_MARKER} found in log."
    grep -E "${EXTRACT_PATTERN}" "${QEMU_LOG}" || true
    exit 0
elif grep -q "${FAIL_MARKER}" "${QEMU_LOG}"; then
    echo "[boot-qemu] FAIL — ${FAIL_MARKER} found in log."
    echo "[boot-qemu] --- Begin QEMU log ---"
    cat "${QEMU_LOG}"
    echo "[boot-qemu] --- End QEMU log ---"
    exit 2
else
    echo "[boot-qemu] FAIL — No test marker found in log (timed out or boot failed)."
    if [[ -s "${QEMU_LOG}" ]]; then
        echo "[boot-qemu] --- Begin QEMU log ---"
        cat "${QEMU_LOG}"
        echo "[boot-qemu] --- End QEMU log ---"
    else
        echo "[boot-qemu] Log file is empty."
    fi
    exit 2
fi
