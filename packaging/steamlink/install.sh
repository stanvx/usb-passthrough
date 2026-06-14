#!/usr/bin/env bash
# Steam Link / Moonlight Companion — AnyPlug Installer
#
# Part of the AnyPlug ecosystem packaging bundle (Issue #18).
# Installs usbip-server on a streaming-client device (Raspberry Pi running
# Steam Link, Moonlight, or Steam Remote Play) so locally-attached controllers
# can be exported to a remote gaming PC over USB/IP.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/stanvx/anyplug/main/packaging/steamlink/install.sh | bash
#   # or run locally:
#   chmod +x install.sh && sudo ./install.sh
#
# Environment variables:
#   ANYPLUG_VERSION      - Release tag to install (default: latest)
#   ANYPLUG_PREFIX       - Install prefix (default: /usr/local/anyplug)
#   ANYPLUG_SKIP_SVC     - Set to "1" to skip systemd / auto-start setup
#   ANYPLUG_NO_STEAM     - Set to "1" to skip Steam-specific hints

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
NO_STEAM="${ANYPLUG_NO_STEAM:-0}"

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
    x86_64|amd64)
      echo "x86_64-linux"
      ;;
    i686|i386|x86)
      echo "i686-linux"
      ;;
    *)
      echo "error: unsupported architecture: ${arch}" >&2
      exit 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Pre-flight
# ---------------------------------------------------------------------------
preflight() {
  # Check for libusb dependency
  if ! ldconfig -p 2>/dev/null | grep -q libusb; then
    echo "warning: libusb not detected. The usbip-server requires libusb-1.0."
    echo "         Install it with: sudo apt-get install libusb-1.0-0  (Debian/Ubuntu)"
    echo "         or the equivalent for your distribution."
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
# Systemd service
# ---------------------------------------------------------------------------
install_systemd_service() {
  local service_name="$1"
  local binary_path="${BIN_DIR}/${service_name}"
  local service_file="${SYSTEMD_DIR}/${service_name}.service"

  cat > "${service_file}" <<-SERVICEEOF
[Unit]
Description=AnyPlug ${service_name} daemon (Streaming Companion)
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

[Install]
WantedBy=multi-user.target
SERVICEEOF

  chmod 644 "${service_file}"
  systemctl daemon-reload
  systemctl enable "${service_name}.service"
  echo "  [anyplug] ${service_name}.service installed and enabled."
}

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
install_config() {
  local conf_file="${CONFIG_DIR}/usbip-server.conf"

  mkdir -p "${CONFIG_DIR}"

  if [[ ! -f "${conf_file}" ]]; then
    cat > "${conf_file}" <<-'CONFEOF'
# AnyPlug server configuration (Steam Link / Moonlight companion)
#
# This device runs usbip-server to export locally-attached controllers
# (gamepads, fight sticks, FFB wheels) to a remote gaming PC over USB/IP.
#
# The gaming PC runs usbip-client to import them, so the game sees them
# as locally attached — no driver conflicts, no input lag workarounds.
#
# Configuration:
interface = 0.0.0.0
port = 3240
announce-mdns = true

# To allowlist specific devices, uncomment and add VID:PID pairs:
#   allow-device = 046d:c242   # Logitech G920
#   allow-device = 054c:09cc   # Sony DualShock 4
#   allow-device = 045e:028e   # Xbox 360 Controller
CONFEOF
    echo "  [anyplug] Created ${conf_file}"
  else
    echo "  [anyplug] ${conf_file} already exists, skipping."
  fi
}

# ---------------------------------------------------------------------------
# Steam / Moonlight helper (non-essential, purely informational)
# ---------------------------------------------------------------------------
print_streaming_hints() {
  if [[ "${NO_STEAM}" -eq 1 ]]; then
    return
  fi

  cat <<-'HINTS'

  ┌─ Steam Remote Play / Moonlight companion hints ─────────────────┐
  │                                                                  │
  │  1. Make sure this device is on the same LAN as your gaming PC.  │
  │                                                                  │
  │  2. Start the server:                                            │
  │       sudo systemctl start usbip-server                          │
  │                                                                  │
  │  3. On your gaming PC, discover available servers:               │
  │       usbip-client --discover                                    │
  │                                                                  │
  │  4. Connect to this device's exported controllers:               │
  │       usbip-client --connect <device-ip>:3240                    │
  │                                                                  │
  │  5. Launch Steam Big Picture / Moonlight and enjoy!              │
  │                                                                  │
  │  The server auto-starts on boot via systemd.                     │
  └──────────────────────────────────────────────────────────────────┘
HINTS
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  local arch_suffix

  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║  AnyPlug — Steam Link / Moonlight Companion Installer   ║"
  echo "║  Export local controllers to your gaming PC via USB/IP  ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""

  preflight

  # Detect architecture
  arch_suffix="$(detect_arch)"
  echo "  [anyplug] Detected architecture: ${arch_suffix}"

  # Create directories
  mkdir -p "${BIN_DIR}" "${CONFIG_DIR}"

  # Download only the server binary (the companion device exports, not imports)
  echo ""
  echo "  [anyplug] Installing usbip-server (companion mode)..."
  download_binary "usbip-server" "${arch_suffix}" "${BIN_DIR}"

  # Install configuration
  echo ""
  echo "  [anyplug] Creating configuration..."
  install_config

  # Install systemd service (unless skipped)
  if [[ "${SKIP_SERVICE}" -ne 1 ]] && systemctl --version &>/dev/null 2>&1; then
    echo ""
    echo "  [anyplug] Installing systemd service..."
    install_systemd_service "usbip-server"
  else
    echo ""
    echo "  [anyplug] Skipping systemd service installation."
  fi

  # Print summary
  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║  Installation complete!                                 ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Binary:  ${BIN_DIR}/usbip-server"
  echo "║  Config:  ${CONFIG_DIR}/usbip-server.conf"
  echo "║  Service: usbip-server.service                         ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  This device now exports USB controllers over TCP:3240  ║"
  echo "║  Connect from your gaming PC with usbip-client.         ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""

  print_streaming_hints
}

main "$@"
