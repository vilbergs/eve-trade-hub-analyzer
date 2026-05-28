#!/usr/bin/env bash
# sync-prod-to-local.sh — dump prod DB and restore into the local docker-compose Postgres.
#
# Usage:
#   ./sync-prod-to-local.sh <PROD_DATABASE_URL>
#
# Example:
#   ./sync-prod-to-local.sh "postgres://user:pass@prod-host:5432/eve_hub?sslmode=require"
#
# The script will:
#   1. pg_dump the prod database (schema + data)
#   2. Drop & recreate the local eve_hub database
#   3. Restore the dump into the local database
#   4. Print row counts so you can verify
#
# Prerequisites:
#   - docker compose postgres running (docker compose up -d)
#   - pg_dump and psql available locally

set -euo pipefail

PROD_URL="${1:?Usage: $0 <PROD_DATABASE_URL>}"

LOCAL_CONTAINER="eve-hub-postgres"
LOCAL_DB="eve_hub"
LOCAL_USER="eve"
LOCAL_PASS="eve"

DUMP_FILE="/tmp/eve_hub_prod_dump.sql"

log() { printf '\n==> %s\n' "$*"; }

# ── 1. Verify local Postgres is running ────────────────────────────
log "Checking local Postgres container..."
if ! docker inspect -f '{{.State.Running}}' "$LOCAL_CONTAINER" 2>/dev/null | grep -q true; then
    echo "Container '$LOCAL_CONTAINER' is not running. Start it with: docker compose up -d"
    exit 1
fi

# ── 2. Dump prod ───────────────────────────────────────────────────
log "Dumping prod database (this may take a moment)..."
pg_dump "$PROD_URL" \
    --no-owner \
    --no-privileges \
    --no-comments \
    --format=plain \
    > "$DUMP_FILE"

DUMP_SIZE=$(du -h "$DUMP_FILE" | cut -f1)
echo "Dump written to $DUMP_FILE ($DUMP_SIZE)"

# ── 3. Drop & recreate local database ─────────────────────────────
log "Recreating local database '$LOCAL_DB'..."
docker exec -e PGPASSWORD="$LOCAL_PASS" "$LOCAL_CONTAINER" \
    psql -U "$LOCAL_USER" -d postgres -c \
    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '$LOCAL_DB' AND pid <> pg_backend_pid();" \
    > /dev/null 2>&1 || true

docker exec -e PGPASSWORD="$LOCAL_PASS" "$LOCAL_CONTAINER" \
    psql -U "$LOCAL_USER" -d postgres -c "DROP DATABASE IF EXISTS $LOCAL_DB;"

docker exec -e PGPASSWORD="$LOCAL_PASS" "$LOCAL_CONTAINER" \
    psql -U "$LOCAL_USER" -d postgres -c "CREATE DATABASE $LOCAL_DB OWNER $LOCAL_USER;"

# ── 4. Restore into local ─────────────────────────────────────────
log "Restoring dump into local database..."
docker exec -i -e PGPASSWORD="$LOCAL_PASS" "$LOCAL_CONTAINER" \
    psql -U "$LOCAL_USER" -d "$LOCAL_DB" -v ON_ERROR_STOP=0 \
    < "$DUMP_FILE"

# ── 5. Print row counts ───────────────────────────────────────────
log "Row counts in local database:"
docker exec -e PGPASSWORD="$LOCAL_PASS" "$LOCAL_CONTAINER" \
    psql -U "$LOCAL_USER" -d "$LOCAL_DB" -c "
SELECT schemaname || '.' || relname AS table,
       n_live_tup AS row_estimate
FROM pg_stat_user_tables
ORDER BY n_live_tup DESC;
"

log "Done! Local database is now a copy of prod."
echo ""
echo "Next steps:"
echo "  1. Run the migration:  DATABASE_URL=postgres://eve:eve@localhost:5432/eve_hub sqlx migrate run"
echo "  2. Verify tables:      docker exec -e PGPASSWORD=eve $LOCAL_CONTAINER psql -U eve -d eve_hub -c '\\dt'"
echo "  3. Run rollup/reports against local to confirm everything works"

rm -f "$DUMP_FILE"
