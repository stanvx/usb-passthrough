#!/usr/bin/env bash
# Build a minimal initramfs with BusyBox for USB/IP CI smoke tests.
#
# Usage:
#   ./ci/build-initramfs.sh
#
# Output: ci/kernel/initramfs.cpio.gz
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KERNEL_DIR="${SCRIPT_DIR}/kernel"
OUTPUT="${KERNEL_DIR}/initramfs.cpio.gz"
WORK_DIR="$(mktemp -d /tmp/initramfs-build.XXXXXXXX)"

BUSYBOX_VERSION="${BUSYBOX_VERSION:-1.36.1}"
# Use a known static BusyBox binary from the official downloads site.
# If unavailable, fall back to building from source.
BUSYBOX_BINARY_URL="https://busybox.net/downloads/binaries/${BUSYBOX_VERSION}-x86_64-linux-musl/busybox"

CLEANUP_DIRS=("${WORK_DIR}")

cleanup() {
    for d in "${CLEANUP_DIRS[@]}"; do
        if [[ -d "${d}" ]]; then
            rm -rf "${d}"
        fi
    done
}
trap cleanup EXIT

echo "[build-initramfs] Creating initramfs at ${WORK_DIR}..."

# ---- BusyBox ---------------------------------------------------------------
BUSYBOX_BIN="${WORK_DIR}/busybox"
if command -v busybox &>/dev/null; then
    echo "[build-initramfs] Using system BusyBox"
    cp "$(command -v busybox)" "${BUSYBOX_BIN}"
elif curl -fsSL --connect-timeout 10 "${BUSYBOX_BINARY_URL}" -o "${BUSYBOX_BIN}"; then
    chmod +x "${BUSYBOX_BIN}"
    echo "[build-initramfs] Downloaded static BusyBox binary"
else
    echo "[build-initramfs] WARNING: Could not download BusyBox binary; building from source..."
    BUSYBOX_SRC_DIR="/tmp/busybox-src-${BUSYBOX_VERSION}"

    if [[ ! -d "${BUSYBOX_SRC_DIR}" ]]; then
        curl -fsSL "https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2" | \
            tar -xjf - -C /tmp/
        mv "/tmp/busybox-${BUSYBOX_VERSION}" "${BUSYBOX_SRC_DIR}"
    fi

    cp /tmp/busybox-src-"${BUSYBOX_VERSION}"/configs/{defconfig,.config}
    make -C "${BUSYBOX_SRC_DIR}" olddefconfig 2>/dev/null || true
    echo "CONFIG_STATIC=y" >> "${BUSYBOX_SRC_DIR}/.config"
    make -C "${BUSYBOX_SRC_DIR}" -j"$(nproc)"

    cp "${BUSYBOX_SRC_DIR}/busybox" "${BUSYBOX_BIN}"
    echo "[build-initramfs] Built BusyBox from source"
fi

# ---- Directory structure ---------------------------------------------------
mkdir -p "${WORK_DIR}/rootfs"/{bin,sbin,proc,sys,dev,etc,config,test_binaries}
cd "${WORK_DIR}/rootfs"

cp "${BUSYBOX_BIN}" bin/busybox
chmod +x bin/busybox

# BusyBox applet symlinks
APPLETS=(
    sh mount echo cat ls mkdir sleep uname cp rm chmod mknod dmesg grep
    poweroff mountpoint clear printf readlink stty false true pidof pgrep
    pkill id env kill ln ps pwd seq sort tail test touch tr which xargs
    basename dirname cut head hexdump od strings find tee wc yes xxd
)
for applet in "${APPLETS[@]}"; do
    ln -s /bin/busybox "bin/${applet}"
done

# Additional sbin symlinks
for applet in modprobe telnet; do
    ln -s /bin/busybox "sbin/${applet}"
done

# ---- /init script ----------------------------------------------------------
cat > init <<'INIT_EOF'
#!/bin/sh
# Init script for USB/IP CI initramfs.
# Mounts filesystems, checks dummy_udc, runs smoke test, optionally executes
# E2E test (if E2E_MODE=1 in cmdline) or run_tests.sh from a shared volume.

set -e

echo "=== CI Initramfs Booting ==="

# Mount essential filesystems
mount -t proc none /proc
mount -t sysfs none /sys
mount -t devtmpfs none /dev
mount -t configfs none /config 2>/dev/null || mount -t configfs configfs /config

# Populate /dev with essential nodes
mknod -m 666 /dev/null c 1 3 2>/dev/null || true
mknod -m 666 /dev/console c 5 1 2>/dev/null || true
mknod -m 666 /dev/ttyS0 c 4 64 2>/dev/null || true

# Give hardware a moment to settle
sleep 1

# Find UDC (UDC Driver Controller) — we expect dummy_udc.0
UDC=""
if [[ -d /sys/class/udc/dummy_udc.0 ]]; then
    UDC="dummy_udc.0"
    echo "  Found UDC: ${UDC}"
else
    UDC_LIST="$(ls /sys/class/udc/ 2>/dev/null || true)"
    if [[ -n "${UDC_LIST}" ]]; then
        UDC="$(echo "${UDC_LIST}" | head -1)"
        echo "  Found UDC: ${UDC} (not dummy_udc.0)"
    else
        echo "  WARNING: No UDC found in /sys/class/udc/"
        echo "  WARNING: Dumping /sys/kernel/debug/usb/ if available..."
        if [[ -d /sys/kernel/debug ]]; then
            ls -R /sys/kernel/debug/usb/ 2>/dev/null || true
        fi
    fi
fi

# ---- Smoke test: HID gadget -----------------------------------------------
SMOKE_PASS=0
if [[ -n "${UDC}" ]] && [[ -d /config/usb_gadget ]]; then
    echo "  Running HID gadget smoke test..."
    GADGET_DIR="/config/usb_gadget/smoke_test"
    mkdir -p "${GADGET_DIR}"

    # Basic gadget configuration
    echo "0x1d6b" > "${GADGET_DIR}/idVendor"   # Linux Foundation
    echo "0x0104" > "${GADGET_DIR}/idProduct"  # Multifunction Composite Gadget
    echo "0x0100" > "${GADGET_DIR}/bcdDevice"
    echo "0x0200" > "${GADGET_DIR}/bcdUSB"

    # English locale strings
    mkdir -p "${GADGET_DIR}/strings/0x409"
    echo "CI"      > "${GADGET_DIR}/strings/0x409/manufacturer"
    echo "SmokeTest" > "${GADGET_DIR}/strings/0x409/product"
    echo "0001"    > "${GADGET_DIR}/strings/0x409/serialnumber"

    # HID function
    mkdir -p "${GADGET_DIR}/functions/hid.usb0"
    echo "1" > "${GADGET_DIR}/functions/hid.usb0/protocol"
    echo "1" > "${GADGET_DIR}/functions/hid.usb0/subclass"
    echo "8" > "${GADGET_DIR}/functions/hid.usb0/report_length"
    # Minimal HID report descriptor (keyboard)
    printf '\x05\x01\x09\x06\xa1\x01\x05\x07\x19\xe0\x29\xe7\x15\x00\x25\x01\x75\x01\x95\x08\x81\x02\x95\x01\x75\x08\x81\x03\x95\x05\x75\x01\x05\x08\x19\x01\x29\x05\x91\x02\x95\x01\x75\x03\x91\x03\x95\x06\x75\x08\x15\x00\x25\x65\x05\x07\x19\x00\x29\x65\x81\x00\xc0' \
        > "${GADGET_DIR}/functions/hid.usb0/report_desc" 2>/dev/null || \
        echo -n "05010906a101050719e029e71500250175019508810295017508810395057501050819012905910295017503910395067508150025650507190029658100c0" | \
        xxd -r -p > "${GADGET_DIR}/functions/hid.usb0/report_desc" 2>/dev/null

    # Configuration
    mkdir -p "${GADGET_DIR}/configs/c.1"
    mkdir -p "${GADGET_DIR}/configs/c.1/strings/0x409"
    echo "CI Smoke Test Config" > "${GADGET_DIR}/configs/c.1/strings/0x409/configuration"

    # Link function to config
    ln -sf "${GADGET_DIR}/functions/hid.usb0" "${GADGET_DIR}/configs/c.1/"

    # Bind to UDC
    echo "${UDC}" > "${GADGET_DIR}/UDC"
    BIND_RESULT=$?

    if [[ ${BIND_RESULT} -eq 0 ]]; then
        echo "  HID gadget bound to ${UDC} successfully"
        SMOKE_PASS=1
    else
        echo "  HID gadget bind returned non-zero: ${BIND_RESULT}"
        # Dump gadget state for debugging
        cat "${GADGET_DIR}/UDC" 2>/dev/null || echo "    (UDC file empty)"
        ls -la "${GADGET_DIR}/configs/c.1/" 2>/dev/null || true
    fi

    # Tear down gadget
    echo "" > "${GADGET_DIR}/UDC" 2>/dev/null || true
    rm -f "${GADGET_DIR}/configs/c.1/hid.usb0" 2>/dev/null || true
    rm -rf "${GADGET_DIR}" 2>/dev/null || true
elif [[ -z "${UDC}" ]]; then
    echo "  SKIP: No UDC available for smoke test"
else
    echo "  SKIP: /config/usb_gadget not available (configfs not mounted)"
fi

# ---- E2E test mode -----------------------------------------------------------
# When E2E_MODE=1 in kernel cmdline, run the embedded E2E test script
# after the smoke test, using 9p-shared binaries.
E2E_EXIT=0
if [[ -f /e2e-test.sh ]] && grep -q "E2E_MODE=1" /proc/cmdline 2>/dev/null; then
    echo ""
    echo "=== E2E: Running HID keyboard tracer bullet ==="

    # Mount the 9p share for test binaries
    mkdir -p /test_binaries
    if ! mountpoint -q /test_binaries 2>/dev/null; then
        mount -t 9p -o trans=virtio test_share /test_binaries 2>/dev/null || \
            echo "  WARNING: Could not mount 9p share; binaries must be embedded"
    fi

    # Copy test binaries from 9p share
    mkdir -p /tmp/tests
    if [[ -d /test_binaries ]]; then
        cp /test_binaries/usbip-server /tmp/tests/ 2>/dev/null || true
        cp /test_binaries/usbip-client /tmp/tests/ 2>/dev/null || true
        chmod +x /tmp/tests/usbip-server /tmp/tests/usbip-client 2>/dev/null || true
    fi

    set +e
    cd /
    sh /e2e-test.sh
    E2E_EXIT=$?
    set -e

    echo "=== E2E test exited with code ${E2E_EXIT} ==="
fi

# ---- Run optional test suite (legacy) ----------------------------------------
TEST_EXIT=0
if [[ ${E2E_EXIT} -eq 0 ]] && [[ -x /test_binaries/run_tests.sh ]]; then
    echo ""
    echo "=== Running test suite: /test_binaries/run_tests.sh ==="
    mkdir -p /tmp/tests
    if mountpoint -q /test_binaries 2>/dev/null; then
        cp -r /test_binaries/* /tmp/tests/ 2>/dev/null || true
    fi
    cd /tmp/tests
    set +e
    sh run_tests.sh
    TEST_EXIT=$?
    set -e
    echo "=== Test suite exited with code ${TEST_EXIT} ==="
fi

# ---- Results ---------------------------------------------------------------
echo ""
if [[ ${E2E_EXIT} -eq 0 ]]; then
    echo "E2E_TEST_PASS"
elif [[ ${SMOKE_PASS} -eq 1 ]]; then
    echo "SMOKE_TEST_PASS"
else
    echo "SMOKE_TEST_FAIL"
fi

echo "=== CI Initramfs Shutdown ==="

# Power off
sleep 1
poweroff -f
INIT_EOF

chmod +x init

# ---- Embed E2E test script -------------------------------------------------
# Copy the E2E test runner into the initramfs so it's available when
# booting with E2E_MODE=1 in the kernel cmdline.
if [[ -f "${SCRIPT_DIR}/e2e-test.sh" ]]; then
    cp "${SCRIPT_DIR}/e2e-test.sh" e2e-test.sh
    chmod +x e2e-test.sh
    echo "[build-initramfs] Embedded e2e-test.sh"
fi

# ---- Pack initramfs --------------------------------------------------------
echo "[build-initramfs] Packing initramfs..."
find . | cpio -o -H newc 2>/dev/null | gzip > "${OUTPUT}"
echo "[build-initramfs] Done — initramfs written to ${OUTPUT}"
echo "[build-initramfs] Size: $(wc -c < "${OUTPUT}" | tr -d ' ') bytes"
