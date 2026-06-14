#!/usr/bin/env bash
# RetroPie AnyPlug Installer
#
# Part of the AnyPlug ecosystem packaging bundle (Issue #18).
# Downloads the usbip-server and usbip-client binaries from GitHub Releases,
# detects the target architecture, installs as a RetroPie supplementary module,
# and sets up systemd auto-start for both daemons.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/stanvx/anyplug/main/packaging/retropie/install.sh | bash
#   # or run locally:
#   chmod +x install.sh && sudo ./install.sh
#
# Environment variables:
#   ANYPLUG_VERSION   - Release tag to install (default: latest)
#   ANYPLUG_PREFIX    - Install prefix (default: /opt/retropie/supplementary/anyplug)
#   ANYPLUG_SKIP_SVC  - Set to "1" to skip systemd unit installation

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
REPO="stanvx/anyplug"
VERSION="${ANYPLUG_VERSION:-latest}"
INSTALL_DIR="${ANYPLUG_PREFIX:-/opt/retropie/supplementary/anyplug}"
CONFIG_DIR="/etc/anyplug"
SYSTEMD_DIR="/etc/systemd/system"
BIN_DIR="${INSTALL_DIR}/bin"
SKIP_SERVICE="${ANYPLUG_SKIP_SVC:-0}"

# RetroPie scriptmodule metadata (used by RetroPie-Setup)
__mod_info=(
  "AnyPlug USB/IP bridge"
  "anyplug"
  "server and client for USB device passthrough"
  "https://github.com/stanvx/anyplug"
  "packaging/retropie"
)

# ---------------------------------------------------------------------------
# Architecture detection
# ---------------------------------------------------------------------------
detect_arch() {
  local arch
  arch="$(uname -m)"
  case "${arch}" in
    x86_64|amd64)
      echo "x86_64-linux"
      ;;
    aarch64|arm64)
      echo "aarch64-linux"
      ;;
    armv7l|armv7|armhf)
      echo "armv7-linux-gnueabihf"
      ;;
    armv6l|armv6)
      echo "armv6-linux-gnueabihf"
      ;;
    riscv64)
      echo "riscv64-linux"
      ;;
    *)
      echo "error: unsupported architecture: ${arch}" >&2
      exit 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Download helpers
# ---------------------------------------------------------------------------
download_binary() {
  local binary_name="$1"   # usbip-server or usbip-client
  local arch_suffix="$2"   # e.g. x86_64-linux
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
    # tarball may contain the full build tree; handle gracefully
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
# Systemd service units
# ---------------------------------------------------------------------------
install_systemd_service() {
  local service_name="$1"   # usbip-server or usbip-client
  local binary_path="${BIN_DIR}/${service_name}"
  local service_file="${SYSTEMD_DIR}/${service_name}.service"

  # Use existing unit if shipped with tarball
  if [[ -f "${INSTALL_DIR}/${service_name}.service" ]]; then
    cp "${INSTALL_DIR}/${service_name}.service" "${service_file}"
  else
    cat > "${service_file}" <<-SERVICEEOF
[Unit]
Description=AnyPlug ${service_name} daemon (RetroPie)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${binary_path} --config ${CONFIG_DIR}/${service_name}.conf
Restart=always
RestartSec=5
User=pi
Group=pi

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=true
MemoryMax=128M

[Install]
WantedBy=multi-user.target
SERVICEEOF
  fi

  chmod 644 "${service_file}"
  systemctl daemon-reload
  systemctl enable "${service_name}.service"
  echo "  [anyplug] ${service_name}.service installed and enabled."
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
# AnyPlug server configuration
# Exports locally-attached USB devices over the network.
# By default, exports all supported devices on interface 0.0.0.0:3240.
#
# For device VID:PID allowlisting, uncomment and add entries:
#   allow-device = 046d:c242   # Logitech G920
#   allow-device = 054c:09cc   # Sony DualShock 4
#
interface = 0.0.0.0
port = 3240
announce-mdns = true
# allow-device =
CONFEOF
        ;;
      usbip-client)
        cat > "${conf_file}" <<-'CONFEOF'
# AnyPlug client configuration
# Imports USB devices from a remote AnyPlug server.
#
# Example:
#   connect = 192.168.1.100:3240
#   auto-reconnect = true
#
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
# Main installation
# ---------------------------------------------------------------------------
main() {
  local arch_suffix

  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║        AnyPlug — RetroPie Installer                     ║"
  echo "║        USB/IP bridge for controller passthrough         ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""

  # Must be root for system-level install
  if [[ $EUID -ne 0 ]]; then
    echo "warning: running as non-root. If the install prefix is not"
    echo "         user-writable you may need sudo."
    echo ""
  fi

  # Detect architecture
  arch_suffix="$(detect_arch)"
  echo "  [anyplug] Detected architecture: ${arch_suffix}"

  # Create install directories
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

  # Install systemd services (unless skipped)
  if [[ "${SKIP_SERVICE}" -ne 1 ]] && systemctl --version &>/dev/null 2>&1; then
    echo ""
    echo "  [anyplug] Installing systemd services..."
    install_systemd_service "usbip-server"
    install_systemd_service "usbip-client"
  else
    echo ""
    echo "  [anyplug] Skipping systemd service installation."
  fi

  # Symlink into RetroPie supplementary path for discoverability
  if [[ -d /opt/retropie/supplementary ]]; then
    ln -sf "${INSTALL_DIR}" "/opt/retropie/supplementary/anyplug" 2>/dev/null || true
  fi

  # Print summary
  echo ""
  echo "╔══════════════════════════════════════════════════════════╗"
  echo "║  Installation complete!                                 ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Binaries:  ${BIN_DIR}"
  echo "║  Config:    ${CONFIG_DIR}"
  echo "║  Services:  usbip-server.service / usbip-client.service ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  Quick start:                                           ║"
  echo "║    sudo systemctl start usbip-server                    ║"
  echo "║    sudo systemctl start usbip-client                    ║"
  echo "║    sudo systemctl status usbip-server                   ║"
  echo "╠══════════════════════════════════════════════════════════╣"
  echo "║  For RetroPie-Setup scriptmodule integration, see:      ║"
  echo "║    https://github.com/stanvx/anyplug/packaging/retropie ║"
  echo "╚══════════════════════════════════════════════════════════╝"
  echo ""
}

main "$@"
