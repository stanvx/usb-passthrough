#!/usr/bin/env bash
# Raspberry Pi OS — AnyPlug Server Setup
#
# Part of the AnyPlug ecosystem packaging bundle (Issue #18).
# Sets up a headless Raspberry Pi (Raspberry Pi OS Bookworm/Bullseye) as a
# dedicated USB device exporter. The Pi runs usbip-server to share locally-
# attached USB devices (controllers, sensors, serial adapters, printers) over
# the network to any AnyPlug client on the same LAN.
#
# This creates the equivalent of a VirtualHere CloudHub using commodity
# hardware and a stock OS — no custom firmware needed.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/stanvx/anyplug/main/packaging/raspberry-pi/server-setup.sh | bash
#   # or run locally:
#   chmod +x server-setup.sh && sudo ./server-setup.sh
#
# Environment variables:
#   ANYPLUG_VERSION        - Release tag to install (default: latest)
#   ANYPLUG_PREFIX         - Install prefix (default: /usr/local/anyplug)
#   ANYPLUG_EXPORT_ALL     - Set to "1" to export all USB devices by default
#   ANYPLUG_DEVICE_IDS     - Space-separated VID:PID pairs to allowlist
#                            (e.g. "046d:c242 054c:09cc")
#   ANYPLUG_SKIP_SVC       - Set to "1" to skip systemd setup
#   ANYPLUG_SKIP_REBOOT    - Set to "1" to skip reboot prompt at end

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
REPO="stanvx/anyplug"
VERSION="${ANYPLUG_VERSION:-latest}"
INSTALL_DIR="${ANYPLUG_PREFIX:-/usr/local/anyplug}"
BIN_DIR="${INSTALL_DIR}/bin"
CONFIG_DIR="/etc/anyplug"
SYSTEMD_DIR="/etc/systemd/system"
SKIP_SERVICE="${ANYPLUG_SKIP_SVC:-0}"
SKIP_REBOOT="${ANYPLUG_SKIP_REBOOT:-0}"
EXPORT_ALL="${ANYPLUG_EXPORT_ALL:-0}"
DEVICE_IDS="${ANYPLUG_DEVICE_IDS:-}"

# ---------------------------------------------------------------------------
# Architecture detection
# ---------------------------------------------------------------------------
detect_arch() {
  local arch
  arch="$(uname -m)"
  case "${arch}" in
    aarch64|arm64)
      echo "aarch64-linux"
      ;;
    armv7l|armv7)
      echo "armv7-linux-gnueabihf"
      ;;
    armv6l|armv6)
      echo "armv6-linux-gnueabihf"
      ;;
    *)
      echo "error: unsupported architecture on Raspberry Pi: ${arch}" >&2
      exit 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
preflight() {
  local errors=0

  # Must be root
  if [[ $EUID -ne 0 ]]; then
    echo "error: This script must be run as root (sudo)." >&2
    exit 1
  fi

  # Check for Raspberry Pi (BCM chip)
  if [[ ! -f "/proc/cpuinfo" ]] || ! grep -qi "BCM\|raspberry pi" /proc/cpuinfo 2>/dev/null; then
    echo "warning: This does not appear to be a Raspberry Pi."
    echo "         The script will continue, but some hardware-specific"
    echo "         steps (USB quirks, dtoverlay) may not apply."
    echo ""
  fi

  # Check OS
  if [[ -f "/etc/os-release" ]]; then
    if ! grep -qi "raspbian\|debian" /etc/os-release 2>/dev/null; then
      echo "warning: This system does not appear to be Raspberry Pi OS."
      echo "         Installation will proceed but package names may differ."
      echo ""
    fi
  fi

  # Check for required packages
  local missing_pkgs=()
  for pkg in libusb-1.0-0 ca-certificates; do
    if ! dpkg -s "${pkg}" &>/dev/null 2>&1; then
      missing_pkgs+=("${pkg}")
    fi
  done

  if [[ ${#missing_pkgs[@]} -gt 0 ]]; then
    echo "  [anyplug] Installing required packages: ${missing_pkgs[*]}..."
    apt-get update -qq
    apt-get install -y -qq "${missing_pkgs[@]}"
  fi

  # Check for kernel VHCI module (needed if this Pi also runs client)
  # Not a hard error — the Pi is typically used as server only.
  if ! lsmod 2>/dev/null | grep -q vhci_hcd; then
    echo "note: VHCI kernel module not loaded. This is expected if this Pi"
    echo "      is used only as a USB device exporter (server)."
    echo ""
  fi
}

# ---------------------------------------------------------------------------
# Download binary
# ---------------------------------------------------------------------------
download_binary() {
  local binary_name="$1"
  local arch_suffix="$2"
  local dest_dir="$3"

  if [[ "${VERSION}" == "latest" ]]; then
    local url="https://github.com/${REPO}/releases/latest/download/${binary_name}-${arch_suffix}.tar.gz"
  else
    local url="https://github.com/${REPO}/releases/download/${VERSION}/${binary_name}-${arch_suffix}.tar.gz"
  fi

  local tarball="/tmp/${binary_name}-${arch_suffix}.tar.gz"

  echo "  [anyplug] Downloading ${binary_name} (${arch_suffix})..."
  if command -v curl &>/dev/null; then
    curl -fsSL "${url}" -o "${tarball}"
  elif command -v wget &>/dev/null; then
    wget -q "${url}" -O "${tarball}"
  else
    echo "error: neither curl nor wget found" >&2
    exit 1
  fi

  echo "  [anyplug] Extracting to ${dest_dir}..."
  mkdir -p "${dest_dir}"
  tar -xzf "${tarball}" -C "${dest_dir}"

  if [[ -f "${dest_dir}/${binary_name}" ]]; then
    chmod +x "${dest_dir}/${binary_name}"
  elif [[ -f "${dest_dir}/target/release/${binary_name}" ]]; then
    mv "${dest_dir}/target/release/${binary_name}" "${dest_dir}/"
    rm -rf "${dest_dir}/target" "${dest_dir}/Cargo."* "${dest_dir}/src" 2>/dev/null || true
    chmod +x "${dest_dir}/${binary_name}"
  else
    echo "error: binary ${binary_name} not found in tarball" >&2
    ls -la "${dest_dir}" >&2
    exit 1
  fi

  rm -f "${tarball}"
  echo "  [anyplug] ${binary_name} installed successfully."
}

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
install_config() {
  local conf_file="${CONFIG_DIR}/usbip-server.conf"

  mkdir -p "${CONFIG_DIR}"

  if [[ ! -f "${conf_file}" ]]; then
    cat > "${conf_file}" <<-CONFEOF
# AnyPlug server configuration (Raspberry Pi OS)
# Generated by server-setup.sh on $(date --iso-8601=seconds)
#
# This Raspberry Pi runs usbip-server to export locally-attached USB devices
# to AnyPlug clients on the network.
#
interface = 0.0.0.0
port = 3240
announce-mdns = true
CONFEOF

    # Add allowlist entries if device IDs were specified
    if [[ -n "${DEVICE_IDS}" ]]; then
      echo "" >> "${conf_file}"
      echo "# Device allowlist (configured via ANYPLUG_DEVICE_IDS)" >> "${conf_file}"
      for id in ${DEVICE_IDS}; do
        echo "allow-device = ${id}" >> "${conf_file}"
      done
    fi

    # Add export-all flag if requested
    if [[ "${EXPORT_ALL}" -eq 1 ]]; then
      echo "" >> "${conf_file}"
      echo "# Export all supported devices (ANYPLUG_EXPORT_ALL=1)" >> "${conf_file}"
      echo "export-all = true" >> "${conf_file}"
    fi

    echo "  [anyplug] Created ${conf_file}"
  else
    echo "  [anyplug] ${conf_file} already exists, skipping."
    echo "  [anyplug] Remove or edit it manually to change settings."
  fi
}

# ---------------------------------------------------------------------------
# Systemd service
# ---------------------------------------------------------------------------
install_systemd_service() {
  local service_name="usbip-server"
  local binary_path="${BIN_DIR}/${service_name}"
  local service_file="${SYSTEMD_DIR}/${service_name}.service"

  cat > "${service_file}" <<-SERVICEEOF
[Unit]
Description=AnyPlug USB server daemon (Raspberry Pi)
Documentation=https://github.com/stanvx/anyplug
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${binary_path} --config ${CONFIG_DIR}/${service_name}.conf
Restart=always
RestartSec=5
User=root

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=true
MemoryMax=64M

[Install]
WantedBy=multi-user.target
SERVICEEOF

  chmod 644 "${service_file}"
  systemctl daemon-reload
  systemctl enable "${service_name}.service"

  echo "  [anyplug] ${service_name}.service installed and enabled."
  echo "  [anyplug] Start now with: systemctl start ${service_name}.service"
}

# ---------------------------------------------------------------------------
# Raspberry Pi-specific optimisations
# ---------------------------------------------------------------------------
apply_pi_optimisations() {
  echo "  [anyplug] Applying Raspberry Pi-specific optimisations..."

  # 1. Ensure USB quirks for DWC2 controller (Pi 0-4) are enabled
  local config_file="/boot/firmware/config.txt"
  if [[ ! -f "${config_file}" ]]; then
    config_file="/boot/config.txt"
  fi

  if [[ -f "${config_file}" ]]; then
    # Enable USB controller quirks if not already present
    if ! grep -q "dwc_otg.fiq_enable" /boot/cmdline.txt 2>/dev/null; then
      echo "  [anyplug] Ensuring USB FIQ is enabled for stable bulk transfers..."
      sed -i 's/$/ dwc_otg.fiq_enable=1 dwc_otg.fiq_fsm_enable=1/' /boot/cmdline.txt 2>/dev/null || true
    fi
  fi

  # 2. Increase USB DMA buffer for isochronous-friendly transfers
  #    (mostly for future-proofing; v1.0 doesn't support isochronous)
  local sysctl_file="/etc/sysctl.d/99-anyplug.conf"
  if [[ ! -f "${sysctl_file}" ]]; then
    cat > "${sysctl_file}" <<-'SYSCTLEOF'
# AnyPlug USB performance tuning (Raspberry Pi)
# Increase network buffer sizes for USB/IP throughput
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216
SYSCTLEOF
    sysctl -p "${sysctl_file}" 2>/dev/null || true
    echo "  [anyplug] Applied network buffer tuning (sysctl)."
  fi

  # 3. Disable USB autosuspend on the Pi (prevents device dropouts)
  local udev_file="/etc/udev/rules.d/99-anyplug-usb-power.rules"
  if [[ ! -f "${udev_file}" ]]; then
    cat > "${udev_file}" <<-'UDEVEOF'
# Disable USB autosuspend for stable AnyPlug operation
ACTION=="add", SUBSYSTEM=="usb", ATTR{power/control}="on"
UDEVEOF
    echo "  [anyplug] Created udev rule to disable USB autosuspend."
  fi

  echo "  [anyplug] Optimisations applied."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  local arch_suffix

  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║    AnyPlug — Raspberry Pi OS Server Setup               ║"
  echo "║    Turn your Pi into a dedicated USB device exporter    ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""

  preflight

  # Detect architecture
  arch_suffix="$(detect_arch)"
  echo "  [anyplug] Detected architecture: ${arch_suffix}"

  # Create directories
  mkdir -p "${BIN_DIR}" "${CONFIG_DIR}"

  # Download server binary only (the Pi exports devices)
  echo ""
  echo "  [anyplug] Installing usbip-server..."
  download_binary "usbip-server" "${arch_suffix}" "${BIN_DIR}"

  # Install configuration
  echo ""
  echo "  [anyplug] Creating configuration..."
  install_config

  # Install systemd service
  if [[ "${SKIP_SERVICE}" -ne 1 ]] && systemctl --version &>/dev/null 2>&1; then
    echo ""
    echo "  [anyplug] Installing systemd service..."
    install_systemd_service
  else
    echo ""
    echo "  [anyplug] Skipping systemd service installation."
  fi

  # Apply Pi-specific optimisations
  echo ""
  apply_pi_optimisations

  # Print summary
  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║  Installation complete!                                 ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Binary:  ${BIN_DIR}/usbip-server"
  echo "║  Config:  ${CONFIG_DIR}/usbip-server.conf"
  echo "║  Service: usbip-server.service                         ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Your Raspberry Pi is now a USB-over-network appliance. ║"
  echo "║  Plug in any supported USB device and it will be        ║"
  echo "║  advertised via mDNS on the LAN.                        ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  if [[ "${SKIP_REBOOT}" -ne 1 ]]; then
    echo "║  A reboot is recommended to apply USB power tuning.     ║"
    echo "║  Reboot now? [Y/n]                                      ║"
  fi
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""

  # Prompt for reboot (unless skipped)
  if [[ "${SKIP_REBOOT}" -ne 1 ]] && [[ -t 0 ]]; then
    read -r -p "  Reboot now? [Y/n]: " reply
    case "${reply}" in
      n|N|no|NO) echo "  [anyplug] Reboot skipped. Reboot later for optimal performance." ;;
      *) echo "  [anyplug] Rebooting..."; reboot ;;
    esac
  fi
}

main "$@"
