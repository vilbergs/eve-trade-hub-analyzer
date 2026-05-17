# eve-trade-hub-analyzer

Rust service that polls a single player-owned EVE Online trade hub (Upwell structure) and the Jita market region via ESI, stores market snapshots in Postgres, and produces stock-health and seeding-opportunity reports.

See `PROMPT.md` for the full build spec, `ADDENDUM.md` for overrides that win against the spec, and `DECISIONS.md` for any additional deviations.

## Quick start (dev)

```sh
# 1. Bring up Postgres
docker compose up -d

# 2. Copy env template and fill in EVE_CLIENT_ID / EVE_CLIENT_SECRET / TOKEN_ENCRYPTION_KEY
cp .env.example .env

# 3. Install sqlx-cli (once)
cargo install sqlx-cli --no-default-features --features rustls,postgres

# 4. Run migrations
sqlx migrate run

# 5. Seed the tracking tables (manual, per ADDENDUM.md §1)
psql "$DATABASE_URL" <<SQL
INSERT INTO tracked_stations (station_id, name) VALUES (1035466617946, 'My Citadel');
INSERT INTO tracked_types (type_id) VALUES (34), (35), (36), (37), (38), (39), (40);
SQL

# 6. Build
cargo build
```

## Binaries

| Binary | Purpose |
|---|---|
| `auth` | One-shot: run the EVE SSO flow and persist a refresh token |
| `sde-sync` | Download the latest Fuzzwork SDE CSVs and load the type/group/market-group tables |
| `poll` | Long-running daemon that snapshots each station in `tracked_stations` plus Jita, filtered by `tracked_types` |
| `rollup` | Roll yesterday's snapshots into `market_daily_agg` and fetch Jita ESI history |
| `report` | Print `stock-health` or `seeding` reports against the stored data |

Invoke any binary with `--help` for its flags.

## Tests

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
