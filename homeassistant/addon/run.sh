#!/usr/bin/env bash
# =============================================================================
# AnyPlug Server — Home Assistant Add-on Entrypoint
# Reads /data/options.json and launches usbip-server with appropriate flags.
# =============================================================================
set -euo pipefail

# --- Defaults ---
PORT="${CONFIG_PORT:-3240}"
METRICS_PORT="${CONFIG_METRICS_PORT:-0}"
ENCRYPTION="${CONFIG_ENCRYPTION:-false}"
ALLOWLIST="${CONFIG_ALLOWLIST:-[]}"

# --- Parse /data/options.json if it exists (HA add-on standard) ---
if [[ -f /data/options.json ]]; then
    PORT="$(jq -r '.port // 3240' /data/options.json)"
    METRICS_PORT="$(jq -r '.metrics_port // 0' /data/options.json)"
    ENCRYPTION="$(jq -r '.encryption // false' /data/options.json)"
    ALLOWLIST="$(jq -r '.allowlist // [] | join(" ")' /data/options.json)"
fi

# --- Build argument list ---
ARGS=("--port" "${PORT}")

# Encryption
if [[ "${ENCRYPTION}" == "true" ]]; then
    ARGS+=("--encrypt")
fi

# Allowlist
if [[ -n "${ALLOWLIST}" && "${ALLOWLIST}" != "[]" ]]; then
    for entry in $(jq -r '.allowlist // [] | .[]' /data/options.json 2>/dev/null); do
        ARGS+=("--allow" "${entry}")
    done
fi

# Metrics / API server
# NOTE: The usbip-server binary currently does not expose a --api-port CLI flag.
# This option is reserved for a future release that will enable the built-in
# axum metrics/health API on the configured port.
if [[ "${METRICS_PORT}" -gt 0 ]]; then
    echo "[ANYPLUG] Metrics port ${METRICS_PORT} configured but requires binary with API support." >&2
    echo "[ANYPLUG] Starting server without metrics endpoint. Set metrics_port: 0 to silence this warning." >&2
    # When the binary supports it:
    # ARGS+=("--api-port" "${METRICS_PORT}")
fi

# --- Start the server ---
exec /usr/local/bin/usbip-server "${ARGS[@]}"
