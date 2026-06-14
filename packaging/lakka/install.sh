#!/usr/bin/env bash
# Lakka AnyPlug Installer
#
# Part of the AnyPlug ecosystem packaging bundle (Issue #18).
# Downloads the usbip-server and usbip-client binaries from GitHub Releases,
# detects the target architecture, and installs into Lakka's writable
# storage partition with systemd service auto-start.
#
# Lakka (LibreELEC-based) uses a read-only squashfs root; persistent binaries
# live under /storage/ (writable overlay). Auto-start is configured via
# /storage/.config/autostart.sh or a systemd drop-in in /storage/.config/system.d/.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/stanvx/anyplug/main/packaging/lakka/install.sh | bash
#   # or run locally after copying to Lakka:
#   chmod +x install.sh && ./install.sh
#
# Environment variables:
#   ANYPLUG_VERSION   - Release tag to install (default: latest)
#   ANYPLUG_PREFIX    - Install prefix (default: /storage/local/anyplug)
#   ANYPLUG_SKIP_SVC  - Set to "1" to skip auto-start setup

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
REPO="stanvx/anyplug"
VERSION="${ANYPLUG_VERSION:-latest}"
INSTALL_DIR="${ANYPLUG_PREFIX:-/storage/local/anyplug}"
BIN_DIR="${INSTALL_DIR}/bin"
CONFIG_DIR="/storage/.config/anyplug"
SYSTEMD_DIR="/storage/.config/system.d"
AUTOSTART_FILE="/storage/.config/autostart.sh"
SKIP_SERVICE="${ANYPLUG_SKIP_SVC:-0}"

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
    *)
      echo "error: unsupported architecture on Lakka: ${arch}" >&2
      exit 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
preflight() {
  # On Lakka, /storage must be writable
  if [[ ! -w "/storage" ]]; then
    echo "error: /storage is not writable. Are you running on Lakka?" >&2
    exit 1
  fi

  # Verify we are on a LibreELEC-based system
  if [[ ! -f "/etc/os-release" ]] || ! grep -qi "libreelec\|lakka" /etc/os-release 2>/dev/null; then
    echo "warning: This system does not appear to be Lakka/LibreELEC."
    echo "         Installation will proceed but auto-start may differ."
  fi
}

# ---------------------------------------------------------------------------
# Download helpers
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
# Auto-start setup
# ---------------------------------------------------------------------------
install_systemd_service() {
  local service_name="$1"
  local binary_path="${BIN_DIR}/${service_name}"
  local service_file="${SYSTEMD_DIR}/${service_name}.service"

  mkdir -p "${SYSTEMD_DIR}"

  cat > "${service_file}" <<-SERVICEEOF
[Unit]
Description=AnyPlug ${service_name} daemon (Lakka)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${binary_path} --config ${CONFIG_DIR}/${service_name}.conf
Restart=always
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
SERVICEEOF

  chmod 644 "${service_file}"
  systemctl daemon-reload 2>/dev/null || true
  systemctl enable "${service_name}.service" 2>/dev/null || true
  echo "  [anyplug] ${service_name}.service installed and enabled."
}

install_autostart_script() {
  local service_name="$1"
  local binary_path="${BIN_DIR}/${service_name}"

  # Append to Lakka's autostart.sh if it doesn't already have the entry
  if [[ -f "${AUTOSTART_FILE}" ]]; then
    if grep -q "${binary_path}" "${AUTOSTART_FILE}" 2>/dev/null; then
      echo "  [anyplug] ${service_name} already in autostart.sh, skipping."
      return
    fi
  fi

  echo "# Start AnyPlug ${service_name}" >> "${AUTOSTART_FILE}"
  echo "${binary_path} --config ${CONFIG_DIR}/${service_name}.conf &" >> "${AUTOSTART_FILE}"
  chmod +x "${AUTOSTART_FILE}" 2>/dev/null || true
  echo "  [anyplug] Added ${service_name} to ${AUTOSTART_FILE}"
}

# ---------------------------------------------------------------------------
# Configuration templates
# ---------------------------------------------------------------------------
install_config() {
  local service_name="$1"
  local conf_file="${CONFIG_DIR}/${service_name}.conf"

  mkdir -p "${CONFIG_DIR}"

  if [[ ! -f "${conf_file}" ]]; then
    case "${service_name}" in
      usbip-server)
        cat > "${conf_file}" <<-'CONFEOF'
# AnyPlug server configuration (Lakka)
# Exports locally-attached controllers to a remote gaming PC.
interface = 0.0.0.0
port = 3240
announce-mdns = true
CONFEOF
        ;;
      usbip-client)
        cat > "${conf_file}" <<-'CONFEOF'
# AnyPlug client configuration (Lakka)
# Imports controllers from a remote server.
# auto-reconnect = true
# connect =
CONFEOF
        ;;
    esac
    echo "  [anyplug] Created ${conf_file}"
  else
    echo "  [anyplug] ${conf_file} already exists, skipping."
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  local arch_suffix

  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║        AnyPlug — Lakka Installer                        ║"
  echo "║        USB/IP bridge for controller passthrough         ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""

  preflight

  # Detect architecture
  arch_suffix="$(detect_arch)"
  echo "  [anyplug] Detected architecture: ${arch_suffix}"

  # Create directories
  mkdir -p "${BIN_DIR}" "${CONFIG_DIR}"

  # Download binaries
  echo ""
  echo "  [anyplug] Installing usbip-server..."
  download_binary "usbip-server" "${arch_suffix}" "${BIN_DIR}"

  echo ""
  echo "  [anyplug] Installing usbip-client..."
  download_binary "usbip-client" "${arch_suffix}" "${BIN_DIR}"

  # Install configuration
  echo ""
  echo "  [anyplug] Creating configuration..."
  install_config "usbip-server"
  install_config "usbip-client"

  # Install auto-start (systemd or autostart.sh)
  if [[ "${SKIP_SERVICE}" -ne 1 ]]; then
    echo ""
    echo "  [anyplug] Configuring auto-start..."

    if systemctl --version &>/dev/null 2>&1; then
      install_systemd_service "usbip-server"
      install_systemd_service "usbip-client"
    fi

    # Always add to autostart.sh as fallback
    install_autostart_script "usbip-server"
    install_autostart_script "usbip-client"
  fi

  # Print summary
  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║  Installation complete!                                 ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Binaries:  ${BIN_DIR}"
  echo "║  Config:    ${CONFIG_DIR}"
  echo "║  Auto-start: ${AUTOSTART_FILE}                         ║"
  echo "║  Services:  ${SYSTEMD_DIR}/*.service                   ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Quick start:                                           ║"
  echo "║    systemctl start usbip-server                         ║"
  echo "║    systemctl start usbip-client                         ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  After reboot, both daemons start automatically.        ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""
}

main "$@"
