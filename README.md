# eve-trade-hub-analyzer

Rust service that polls a single player-owned EVE Online trade hub (Upwell structure) and the Jita market region via ESI, stores market snapshots in Postgres, and produces stock-health and seeding-opportunity reports.

See `PROMPT.md` for the full build spec, `ADDENDUM.md` for overrides that win against the spec, and `DECISIONS.md` for any additional deviations.

## Quick start (dev)

```sh
# 1. Bring up Postgres
docker compose up -d

# 2. Copy env template and fill in EVE_CLIENT_ID / EVE_CLIENT_SECRET
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

The `tests/auth_flow_test.rs` integration tests require `DATABASE_URL`
to point at a running Postgres (the dev `docker compose up -d` is
enough). When `DATABASE_URL` is unset they print a skip notice and
return.

## Reports

```sh
# Stock health across every tracked station; table to stdout
cargo run --bin report -- stock-health

# Restrict to one station; CSV for piping
cargo run --bin report -- stock-health --station 1035466617946 --format csv

# Seeding opportunities with a profit floor
cargo run --bin report -- seeding --min-profit-per-day 1000000 --format json
```

## Deploying to a Raspberry Pi

The repo ships systemd units in `deploy/` and the binaries are
single-file, so deployment is `cross build` → `scp` → enable units.

1. **Cross-compile on the dev host.**

   ```sh
   cargo install cross --locked
   cross build --release --target aarch64-unknown-linux-gnu \
       --bin auth --bin sde-sync --bin poll --bin rollup --bin report
   ```

   Output lands in `target/aarch64-unknown-linux-gnu/release/`.

2. **One-time host setup on the Pi.**

   ```sh
   sudo useradd --system --create-home --home-dir /var/lib/eve-trade-hub-analyzer eve-hub
   sudo mkdir -p /etc/eve-trade-hub-analyzer
   sudo install -o eve-hub -g eve-hub -m 0600 deploy/env.example \
       /etc/eve-trade-hub-analyzer/env
   sudoedit /etc/eve-trade-hub-analyzer/env   # fill in real values
   ```

3. **Install binaries.**

   ```sh
   sudo install -o root -g root -m 0755 \
       target/aarch64-unknown-linux-gnu/release/{auth,sde-sync,poll,rollup,report} \
       /usr/local/bin/
   ```

4. **Apply migrations.** Either install sqlx-cli on the Pi and run
   `sqlx migrate run`, or run migrations from the dev host against the
   managed DB before installing.

5. **Seed tracked tables.** Same `INSERT` snippet as the dev quick-start
   above.

6. **Link a character.** Run `auth` on the Pi (or any machine that can
   reach `EVE_CALLBACK_URL`) once; it writes the `characters` row and
   exits.

7. **Enable units.**

   ```sh
   sudo install -m 0644 deploy/eve-trade-hub-*.service /etc/systemd/system/
   sudo install -m 0644 deploy/eve-trade-hub-*.timer /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable --now eve-trade-hub-poll.service
   sudo systemctl enable --now eve-trade-hub-rollup.timer
   sudo systemctl enable --now eve-trade-hub-sde-sync.timer
   ```

8. **Observe.** `journalctl -fu eve-trade-hub-poll` for the daemon;
   `systemctl list-timers` for next-run schedule.

