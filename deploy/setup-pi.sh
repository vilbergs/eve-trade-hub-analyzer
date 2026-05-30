#!/usr/bin/env bash
# Host-side setup for the eve-trade-hub-analyzer on a Raspberry Pi.
# Idempotent: safe to re-run when shipping new binaries or unit files.
#
# Out of scope (do these by hand — they need credentials or interaction):
#   * sqlx migrate run
#   * seeding tracked_stations / tracked_types
#   * `auth` (interactive OAuth flow)
#   * `systemctl enable --now` (deferred until /etc/.../env is filled)
#
# Usage:
#   deploy/setup-pi.sh          # build + sudo install
#   deploy/setup-pi.sh --no-build  # sudo install only (binaries must exist)
#   BIN_DIR=/tmp/bins deploy/setup-pi.sh --no-build

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_PATH="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

BIN_DIR="${BIN_DIR:-target/release}"
SERVICE_USER="${SERVICE_USER:-eve-hub}"
SERVICE_HOME="${SERVICE_HOME:-/var/lib/eve-trade-hub-analyzer}"
ETC_DIR="${ETC_DIR:-/etc/eve-trade-hub-analyzer}"
BINARIES=(auth sde-sync poll rollup report)
SKIP_BUILD=0

for arg in "$@"; do
    case "$arg" in
        --no-build) SKIP_BUILD=1 ;;
        *) printf 'unknown flag: %s\n' "$arg" >&2; exit 1 ;;
    esac
done

env_was_created=0

log() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

build_binaries() {
    if (( SKIP_BUILD )); then
        log "skipping build (--no-build)"
        return
    fi
    log "building release binaries"
    local bin_flags=()
    for b in "${BINARIES[@]}"; do
        bin_flags+=(--bin "$b")
    done
    cargo build --release "${bin_flags[@]}"
}

require_root() {
    [[ $EUID -eq 0 ]] || die "must be run as root (try: sudo $SCRIPT_PATH)"
}

require_binaries() {
    local missing=()
    for b in "${BINARIES[@]}"; do
        [[ -x "$BIN_DIR/$b" ]] || missing+=("$BIN_DIR/$b")
    done
    if (( ${#missing[@]} )); then
        printf 'error: binaries not found:\n' >&2
        printf '  %s\n' "${missing[@]}" >&2
        printf 'hint: build first or drop --no-build\n' >&2
        exit 1
    fi
}

ensure_user() {
    if id -u "$SERVICE_USER" &>/dev/null; then
        log "user $SERVICE_USER already exists"
    else
        log "creating system user $SERVICE_USER (home $SERVICE_HOME)"
        useradd --system --create-home --home-dir "$SERVICE_HOME" \
            --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
}

ensure_env() {
    install -d -m 0755 "$ETC_DIR"
    if [[ -f "$ETC_DIR/env" ]]; then
        log "$ETC_DIR/env already present — leaving in place"
    else
        log "installing $ETC_DIR/env from deploy/env.example (PLACEHOLDER VALUES)"
        install -o "$SERVICE_USER" -g "$SERVICE_USER" -m 0600 \
            deploy/env.example "$ETC_DIR/env"
        env_was_created=1
    fi
}

install_binaries() {
    log "installing binaries to /usr/local/bin"
    for b in "${BINARIES[@]}"; do
        install -o root -g root -m 0755 "$BIN_DIR/$b" "/usr/local/bin/$b"
    done
}

install_units() {
    log "installing systemd units to /etc/systemd/system"
    install -m 0644 deploy/eve-trade-hub-*.service /etc/systemd/system/
    install -m 0644 deploy/eve-trade-hub-*.timer   /etc/systemd/system/
    systemctl daemon-reload
    # If the poll service was already running, pick up the new binary.
    systemctl try-restart eve-trade-hub-poll.service || true
}

print_next_steps() {
    cat <<EOF

setup complete.

next steps (manual):
  1. edit ${ETC_DIR}/env — fill DATABASE_URL, EVE_CLIENT_ID, EVE_CLIENT_SECRET, EVE_USER_AGENT
       sudoedit ${ETC_DIR}/env
EOF
    if (( env_was_created )); then
        echo "       (just installed from the template — values are placeholders)"
    fi
    cat <<EOF
  2. apply migrations (run once, from dev host or here if sqlx-cli installed):
       sqlx migrate run --database-url "\$DATABASE_URL"
  3. seed tracked_stations / tracked_types (see README §Quick start step 5)
  4. link an EVE character (interactive, opens browser):
       sudo -u ${SERVICE_USER} env \$(grep -v '^#' ${ETC_DIR}/env | xargs) /usr/local/bin/auth
  5. enable services:
       systemctl enable --now eve-trade-hub-poll.service
       systemctl enable --now eve-trade-hub-rollup.timer
       systemctl enable --now eve-trade-hub-sde-sync.timer
  6. observe:
       journalctl -fu eve-trade-hub-poll
       systemctl list-timers 'eve-trade-hub-*'
EOF
}

main() {
    # Build as the current user (cargo needs ~/.cargo), then escalate for install.
    build_binaries
    if [[ $EUID -ne 0 ]]; then
        log "re-running as root for install"
        exec sudo --preserve-env=BIN_DIR,SERVICE_USER,SERVICE_HOME,ETC_DIR \
            "$SCRIPT_PATH" --no-build
    fi
    require_binaries
    ensure_user
    ensure_env
    install_binaries
    install_units
    print_next_steps
}

main "$@"
