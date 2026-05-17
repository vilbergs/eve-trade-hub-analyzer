# eve-trade-hub-analyzer

Rust service that polls a single player-owned EVE Online trade hub (Upwell structure) and the Jita market region via ESI, stores market snapshots in Postgres, and produces stock-health and seeding-opportunity reports.

See `PROMPT.md` for the full build spec and `DECISIONS.md` for any deviations.

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

# 5. Build
cargo build
```

## Binaries

| Binary | Purpose |
|---|---|
| `auth` | One-shot: run the EVE SSO flow and persist an encrypted refresh token |
| `discover` | List Upwell structures the linked character can dock at |
| `sde-sync` | Download the latest Fuzzwork SDE CSVs and load the type/group/market-group tables |
| `poll` | Long-running daemon that snapshots hub + Jita market orders on an interval |
| `rollup` | Roll yesterday's snapshots into `market_daily_agg` and fetch Jita ESI history |
| `report` | Print `stock-health` or `seeding` reports against the stored data |

Invoke any binary with `--help` for its flags.

## Tests

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
