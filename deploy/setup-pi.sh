#!/usr/bin/env bash
# Single-command deploy for eve-trade-hub-analyzer on a Raspberry Pi.
# Idempotent end-to-end: build → install → migrate → link character → start.
#
# First run on a clean host installs the env file from the template (PLACEHOLDER
# values) and stops, asking you to fill in the secrets. Fill them, then re-run:
# it applies migrations, links an EVE character (interactive — unless one is
# already linked, or --no-auth), and enables + starts the poll service and the
# rollup / sde-sync timers. SDE reference data loads on the first sde-sync timer
# tick (or force it now: systemctl start eve-trade-hub-sde-sync.service).
#
# Out of scope (do these by hand — operational data this script must not touch):
#   * seeding tracked_stations / tracked_types
#
# Usage:
#   deploy/setup-pi.sh                 # full deploy
#   deploy/setup-pi.sh --no-build      # skip cargo build (binaries must exist)
#   deploy/setup-pi.sh --no-auth       # never run the interactive character link
#   deploy/setup-pi.sh --no-start      # install + migrate, but don't enable/start units
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
# migrate must be built/installed too — it applies the embedded migrations.
BINARIES=(migrate auth sde-sync poll rollup report)

SKIP_BUILD=0
NO_AUTH=0
NO_START=0

for arg in "$@"; do
    case "$arg" in
        --no-build) SKIP_BUILD=1 ;;
        --no-auth)  NO_AUTH=1 ;;
        --no-start) NO_START=1 ;;
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
}

# Parse the systemd EnvironmentFile literally (no shell eval — values may
# contain spaces or parens, e.g. EVE_USER_AGENT) and export each KEY=VALUE.
load_env_file() {
    local f="$ETC_DIR/env" key val
    [[ -r "$f" ]] || die "cannot read $f"
    while IFS='=' read -r key val; do
        [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
        val="${val%\"}"; val="${val#\"}"   # strip optional surrounding quotes
        export "$key=$val"
    done < "$f"
}

# True once the operator has replaced the template's placeholders.
env_is_filled() {
    [[ -n "${DATABASE_URL:-}" && "$DATABASE_URL" != *"user:password@host"* ]] || return 1
    [[ -n "${EVE_CLIENT_ID:-}" && -n "${EVE_CLIENT_SECRET:-}" ]] || return 1
}

print_fill_env_and_stop() {
    cat <<EOF

env not yet configured — installation scaffolded, bring-up deferred.

fill in the real values, then re-run this script to finish:
  sudoedit ${ETC_DIR}/env
    DATABASE_URL        real Postgres connection string
    EVE_CLIENT_ID       from https://developers.eveonline.com
    EVE_CLIENT_SECRET
    EVE_USER_AGENT      include a contact email
  sudo ${SCRIPT_PATH}
EOF
}

run_migrations() {
    log "applying database migrations"
    /usr/local/bin/migrate
}

# 0 = a character is linked, 1 = none linked, 2 = can't tell (no psql / no DB).
character_linked() {
    command -v psql >/dev/null || return 2
    local n
    n="$(psql "$DATABASE_URL" -tA -c 'SELECT count(*) FROM characters' 2>/dev/null)" || return 2
    [[ "${n:-0}" -gt 0 ]]
}

maybe_link_character() {
    if (( NO_AUTH )); then
        log "skipping character link (--no-auth)"
        return
    fi
    local rc=0
    character_linked || rc=$?
    case "$rc" in
        0) log "an EVE character is already linked — skipping auth"; return ;;
        2) log "cannot check linked characters (psql unavailable) — skipping auto-auth"
           log "    link later by re-running this script from an interactive shell"
           return ;;
    esac
    if [[ ! -t 0 ]]; then
        log "no controlling terminal — skipping interactive auth"
        log "    link later by re-running this script from an interactive shell"
        return
    fi
    log "no EVE character linked — starting interactive SSO login"
    log "    open the URL it prints in a browser (SSH-forward the callback port if headless)"
    /usr/local/bin/auth
}

enable_and_start() {
    if (( NO_START )); then
        log "skipping service start (--no-start)"
        return
    fi
    log "enabling + starting poll service and timers"
    systemctl enable eve-trade-hub-poll.service
    # restart (not just start) so a re-deploy picks up the freshly installed binary.
    systemctl restart eve-trade-hub-poll.service
    systemctl enable --now eve-trade-hub-rollup.timer
    systemctl enable --now eve-trade-hub-sde-sync.timer
}

print_status() {
    cat <<EOF

deploy complete.

still manual (out of scope — your operational data):
  * seed tracked_stations / tracked_types (see README §Quick start step 5)

SDE reference data loads on the first sde-sync tick; force it now with:
  systemctl start eve-trade-hub-sde-sync.service

observe:
  journalctl -fu eve-trade-hub-poll
  systemctl list-timers 'eve-trade-hub-*'
EOF
}

main() {
    # Build as the invoking user (cargo needs ~/.cargo), then escalate to install
    # and bring up. Forward the bring-up flags across the sudo re-exec.
    build_binaries
    if [[ $EUID -ne 0 ]]; then
        log "re-running as root for install + bring-up"
        local forward=(--no-build)
        (( NO_AUTH ))  && forward+=(--no-auth)
        (( NO_START )) && forward+=(--no-start)
        exec sudo --preserve-env=BIN_DIR,SERVICE_USER,SERVICE_HOME,ETC_DIR \
            "$SCRIPT_PATH" "${forward[@]}"
    fi

    require_binaries
    ensure_user
    ensure_env
    install_binaries
    install_units

    load_env_file
    if ! env_is_filled; then
        print_fill_env_and_stop
        exit 0
    fi

    run_migrations
    maybe_link_character
    enable_and_start
    print_status
}

main "$@"
